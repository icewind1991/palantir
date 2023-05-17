pub mod disk;
pub mod gpu;
pub mod hwmon;
pub mod power;
pub mod sensors;

use self::disk::zfs::pools;
use self::disk::*;
use self::sensors::*;
use crate::linux::disk::zfs::arcstats;
use crate::linux::gpu::{update_gpu_power, utilization};
use crate::linux::power::power_usage;
use crate::{hostname, Error, MultiSensorSource, Result, SensorData, SensorSource};
use std::fmt::Write;
use std::sync::Mutex;
use sysconf::SysconfError;

impl From<SysconfError> for Error {
    fn from(_: SysconfError) -> Self {
        Error::Other("Unsupported sysconf".into())
    }
}

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
        std::thread::spawn(update_gpu_power);

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

    if let Some(power) = power_usage()? {
        power.write(&mut result, &sensors.hostname);
    }
    if let Some(arc) = arcstats()? {
        arc.write(&mut result, &sensors.hostname);
    }
    if let Some(memory) = gpu::memory() {
        memory.write(&mut result, &sensors.hostname)
    }

    for usage in utilization() {
        usage.write(&mut result, &sensors.hostname);
    }

    Ok(result)
}
