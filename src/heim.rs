use color_eyre::Result;
use futures_util::stream::{Stream, StreamExt};
use futures_util::{future, TryStreamExt};
use heim::units::{information, ratio, thermodynamic_temperature};
use std::time::Duration;
use tokio::time::sleep;

#[derive(Debug, Clone)]
pub enum TemperatureLabel {
    CPU,
}

#[derive(Debug, Clone)]
pub struct Temperature {
    pub sensor: TemperatureLabel,
    pub temperature: f32,
}

#[derive(Debug, Clone)]
pub struct Memory {
    pub total: u64,
    pub free: u64,
    pub available: u64,
}

#[derive(Debug, Clone, Default)]
pub struct NetworkStats {
    pub interface: String,
    pub bytes_sent: u64,
    pub bytes_received: u64,
}

#[derive(Default)]
pub struct Heim {}

impl Heim {
    #[allow(dead_code)]
    pub async fn temperatures() -> Result<Vec<Temperature>> {
        let mut temperatures = Vec::new();
        let results: Vec<_> = heim::sensors::temperatures().try_collect().await?;
        // let results: Vec<TemperatureSensor> = Vec::new();
        // pin_mut!(results);
        for sensor in results {
            if let Some(temp) = match (sensor.unit(), sensor.label()) {
                ("k10temp", Some("Tdie")) => Some(Temperature {
                    sensor: TemperatureLabel::CPU,
                    temperature: sensor
                        .current()
                        .get::<thermodynamic_temperature::degree_celsius>(),
                }),
                _ => None,
            } {
                temperatures.push(temp);
            }
        }
        Ok(temperatures)
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

    pub async fn network_stats(&self) -> Result<impl Stream<Item = NetworkStats>> {
        let networks = heim::net::io_counters().await?;
        Ok(networks
            .filter_map(|network| future::ready(network.ok()))
            .filter(|network| future::ready(network.interface().starts_with("enp")))
            .map(|network| NetworkStats {
                interface: network.interface().into(),
                bytes_sent: network.bytes_sent().get::<information::byte>(),
                bytes_received: network.bytes_recv().get::<information::byte>(),
            }))
    }

    pub async fn hostname(&self) -> Result<String> {
        Ok(heim::host::platform().await?.hostname().to_string())
    }
}
