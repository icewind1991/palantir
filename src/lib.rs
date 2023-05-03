pub mod disk;
pub mod docker;
pub mod gpu;
pub mod hwmon;
pub mod power;
pub mod sensors;

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
    mem: Mutex<MemorySource>,
    disk_stats: Mutex<DiskStatSource>,
    disk_usage: Mutex<DiskUsageSource>,
}

impl Sensors {
    pub fn new() -> Result<Sensors> {
        Ok(Sensors {
            hostname: hostname()?,
            cpu: Mutex::new(CpuTimeSource::new()?),
            temp: Mutex::new(TemperatureSource::new()?),
            net: Mutex::new(NetworkSource::new()?),
            mem: Mutex::new(MemorySource::new()?),
            disk_stats: Mutex::new(DiskStatSource::new()?),
            disk_usage: Mutex::new(DiskUsageSource::new()?),
        })
    }
}

pub fn get_metrics(sensors: &Sensors) -> Result<String> {
    let hostname = &sensors.hostname;
    let mut disk_source = sensors.disk_stats.lock().unwrap();
    let mut disk_usage_source = sensors.disk_usage.lock().unwrap();
    let disks = disk_source.read()?;
    let disk_usage = disk_usage_source.read()?;
    let cpu = sensors.cpu.lock().unwrap().read()?;
    let memory = sensors.mem.lock().unwrap().read()?;
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
        if let Ok(disk) = disk {
            disk.write(&mut result, hostname);
        }
    }

    for disk in disk_usage {
        if let Ok(disk) = disk {
            disk.write(&mut result, hostname);
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
