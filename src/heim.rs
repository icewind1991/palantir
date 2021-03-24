use color_eyre::eyre::WrapErr;
use color_eyre::Result;
use futures_util::future;
use futures_util::stream::{Stream, StreamExt};
use heim::disk::{FileSystem, Partition};
use heim::sensors::TemperatureSensor;
use heim::units::{information, ratio, thermodynamic_temperature};
use once_cell::sync::Lazy;
use parse_display::Display;
use regex::Regex;
use std::collections::HashMap;
use std::time::Duration;
use tokio::time::sleep;

#[derive(Debug, Clone, Hash, Eq, PartialEq, Display)]
#[display(style = "lowercase")]
pub enum TemperatureLabel {
    CPU,
}

#[derive(Debug, Clone)]
pub struct Memory {
    pub total: u64,
    pub free: u64,
    pub available: u64,
}

#[derive(Debug, Clone, Default)]
pub struct IOStats {
    pub interface: String,
    pub bytes_sent: u64,
    pub bytes_received: u64,
}

#[derive(Clone, Debug)]
pub struct DiskUsage {
    pub name: String,
    pub size: u64,
    pub free: u64,
}

#[derive(Default)]
pub struct Heim {}

impl Heim {
    pub async fn temperatures(&self) -> Result<HashMap<TemperatureLabel, f32>> {
        // ugly workaround problems between async-fs and tokio
        let results = tokio::task::spawn_blocking(|| {
            futures_lite::future::block_on(
                heim::sensors::temperatures()
                    .collect::<Vec<Result<TemperatureSensor, heim::Error>>>(),
            )
        })
        .await
        .wrap_err("Failed to resolve future")?
        .into_iter()
        .filter_map(|result| result.ok())
        .filter_map(|sensor| match (sensor.unit(), sensor.label()) {
            ("k10temp", Some("Tdie")) => Some((
                TemperatureLabel::CPU,
                sensor
                    .current()
                    .get::<thermodynamic_temperature::degree_celsius>(),
            )),
            _ => None,
        });
        Ok(results.collect())
    }

    pub async fn memory(&self) -> Result<Memory> {
        let memory = heim::memory::memory().await?;
        Ok(Memory {
            total: memory.total().get::<information::byte>(),
            free: memory.free().get::<information::byte>(),
            available: memory.available().get::<information::byte>(),
        })
    }

    pub async fn cpu_usage(&self) -> Result<f32> {
        let cores = heim::cpu::logical_count().await?;
        let measurement_1 = heim::cpu::usage().await?;
        sleep(Duration::from_millis(100)).await;
        let measurement_2 = heim::cpu::usage().await?;
        Ok((measurement_2 - measurement_1).get::<ratio::percent>() / cores as f32)
    }

    pub async fn network_stats(&self) -> Result<impl Stream<Item = IOStats>> {
        let networks = heim::net::io_counters().await?;
        Ok(networks
            .filter_map(|network| future::ready(network.ok()))
            .filter(|network| future::ready(network.interface().starts_with("enp")))
            .map(|network| IOStats {
                interface: network.interface().into(),
                bytes_sent: network.bytes_sent().get::<information::byte>(),
                bytes_received: network.bytes_recv().get::<information::byte>(),
            }))
    }

    pub async fn hostname(&self) -> Result<String> {
        Ok(heim::host::platform().await?.hostname().to_string())
    }

    pub async fn disk_stats(&self) -> Result<impl Stream<Item = IOStats>> {
        static DISK_REGEX: Lazy<Regex> =
            Lazy::new(|| Regex::new(r"^([sv]d[a-z]+|nvme\dn\d)$").unwrap());
        let disks = heim::disk::io_counters().await?;
        Ok(disks
            .filter_map(|disk| future::ready(disk.ok()))
            .filter_map(|disk| {
                future::ready(
                    disk.device_name()
                        .to_str()
                        .map(str::to_string)
                        .map(|name| (disk, name)),
                )
            })
            .filter(|(_disk, name)| future::ready(DISK_REGEX.is_match(&name)))
            .map(|(disk, name)| IOStats {
                interface: name,
                bytes_sent: disk.write_bytes().get::<information::byte>(),
                bytes_received: disk.read_bytes().get::<information::byte>(),
            }))
    }

    pub async fn disk_usage(&self) -> Result<impl Stream<Item = DiskUsage>> {
        Ok(heim::disk::partitions_physical()
            .await?
            .filter_map(|result| future::ready(result.ok()))
            .filter(|partition: &Partition| {
                future::ready(!partition.file_system().eq(&FileSystem::Zfs))
            })
            .filter_map(|partition: Partition| async move {
                let name = partition.mount_point().to_string_lossy().to_string();
                partition.usage().await.ok().map(|usage| (name, usage))
            })
            .map(|(mount_point, usage)| DiskUsage {
                name: mount_point,
                size: usage.total().get::<information::byte>(),
                free: usage.free().get::<information::byte>(),
            }))
    }
}
