pub mod disk;
pub mod docker;
pub mod gpu;
pub mod hwmon;
pub mod power;
pub mod sensors;

use crate::disk::disk_usage;
use crate::disk::zfs::pools;
use crate::disk::*;
use crate::sensors::*;
use std::ffi::NulError;
use std::fmt::Write;
use std::io;
use std::num::{ParseFloatError, ParseIntError};
use std::str::Utf8Error;
use sysconf::SysconfError;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error("Unsupported sysconf")]
    Sysconf(SysconfError),
    #[error("Non UTF8 hostname")]
    InvalidHostName,
    #[error(transparent)]
    InvalidIntData(#[from] ParseIntError),
    #[error(transparent)]
    InvalidFloatData(#[from] ParseFloatError),
    #[error(transparent)]
    InvalidStringData(#[from] Utf8Error),
    #[error(transparent)]
    InvalidCStringData(#[from] NulError),
    #[error("Failed to query vfs stats")]
    StatVfs,
}

impl From<SysconfError> for Error {
    fn from(value: SysconfError) -> Self {
        Error::Sysconf(value)
    }
}

pub type Result<T, E = Error> = std::result::Result<T, E>;

pub fn get_metrics() -> Result<String> {
    let disk_usage = disk_usage()?;
    let disks = disk_stats()?;
    let mut cpu_source = CpuTimeSource::new()?;
    let cpu = cpu_source.read()?;
    let hostname = hostname()?;
    let memory = memory()?;
    let mut temp_source = TemperatureSource::new()?;
    let temperatures = temp_source.read()?;
    let pools = pools();
    let networks = network_stats()?;
    let mut result = String::with_capacity(256);

    cpu.write(&mut result, &hostname);
    memory.write(&mut result, &hostname);

    for pool in pools {
        writeln!(
            &mut result,
            "zfs_pool_size{{host=\"{}\", pool=\"{}\"}} {}",
            hostname, pool.name, pool.size
        )
        .ok();
        writeln!(
            &mut result,
            "zfs_pool_free{{host=\"{}\", pool=\"{}\"}} {}",
            hostname, pool.name, pool.free
        )
        .ok();
    }
    for network in networks {
        if network.bytes_received > 0 || network.bytes_sent > 0 {
            writeln!(
                &mut result,
                "net_sent{{host=\"{}\", network=\"{}\"}} {}",
                hostname, network.interface, network.bytes_sent
            )
            .ok();
            writeln!(
                &mut result,
                "net_received{{host=\"{}\", network=\"{}\"}} {}",
                hostname, network.interface, network.bytes_received
            )
            .ok();
        }
    }
    for disk in disks {
        if disk.bytes_received > 0 && disk.bytes_sent > 0 {
            writeln!(
                &mut result,
                "disk_sent{{host=\"{}\", disk=\"{}\"}} {}",
                hostname, disk.interface, disk.bytes_sent
            )
            .ok();
            writeln!(
                &mut result,
                "disk_received{{host=\"{}\", disk=\"{}\"}} {}",
                hostname, disk.interface, disk.bytes_received
            )
            .ok();
        }
    }

    for disk in disk_usage {
        if disk.size > 0 {
            writeln!(
                &mut result,
                "disk_size{{host=\"{}\", disk=\"{}\"}} {}",
                hostname, disk.name, disk.size
            )
            .ok();
            writeln!(
                &mut result,
                "disk_free{{host=\"{}\", disk=\"{}\"}} {}",
                hostname, disk.name, disk.free
            )
            .ok();
        }
    }
    for (label, temp) in temperatures {
        if temp != 0.0 {
            writeln!(
                &mut result,
                "temperature{{host=\"{}\", sensor=\"{}\"}} {:.1}",
                hostname, label, temp
            )
            .ok();
        }
    }
    Ok(result)
}

pub trait SensorData {
    /// Write sensor data in prometheus compatible format
    fn write<W: Write>(&self, w: W, hostname: &str);
}

pub trait SensorSource {
    type Data: SensorData;

    fn read(&mut self) -> Result<Self::Data>;
}
