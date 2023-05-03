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
use std::sync::Mutex;
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

pub struct Sensors {
    pub hostname: String,
    cpu: Mutex<CpuTimeSource>,
    temp: Mutex<TemperatureSource>,
    net: Mutex<NetworkSource>,
}

impl Sensors {
    pub fn new() -> Result<Sensors> {
        Ok(Sensors {
            hostname: hostname()?,
            cpu: Mutex::new(CpuTimeSource::new()?),
            temp: Mutex::new(TemperatureSource::new()?),
            net: Mutex::new(NetworkSource::new()?),
        })
    }
}

pub fn get_metrics(sensors: &Sensors) -> Result<String> {
    let hostname = &sensors.hostname;
    let disk_usage = disk_usage()?;
    let disks = disk_stats()?;
    let cpu = sensors.cpu.lock().unwrap().read()?;
    let memory = memory()?;
    let temperatures = sensors.temp.lock().unwrap().read()?;
    let mut net = sensors.net.lock().unwrap();
    let networks = net.read()?;
    let pools = pools();
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
        if let Ok(network) = network {
            network.write(&mut result, &hostname);
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

pub trait MultiSensorSource {
    type Data: SensorData;
    type Iter<'a>: Iterator<Item = Result<Self::Data>>
    where
        Self: 'a;

    fn read(&mut self) -> Result<Self::Iter<'_>>;
}
