mod cpu;
mod disk;
mod reg;
mod wmi;

use self::cpu::CpuTimeSource;
use crate::data::{DiskUsage, GpuMemory, GpuUsage, Memory, NetStats};
use crate::win::wmi::WmiSensor;
use crate::Result;
use crate::{hostname, SensorData, SensorSource};
use once_cell::sync::Lazy;
use os_thread_local::ThreadLocal;
use std::borrow::Cow;
use std::sync::Mutex;
use sysinfo::{ComponentExt, DiskExt, NetworkExt, System, SystemExt};

pub struct Sensors {
    pub hostname: String,
    pub system: Mutex<System>,
    cpu: Mutex<CpuTimeSource>,
    gpu_mem_total: u64,
}

static WMI: Lazy<ThreadLocal<WmiSensor>> =
    Lazy::new(|| ThreadLocal::new(|| WmiSensor::new().expect("failed to init wmi")));

impl Sensors {
    pub fn new() -> Result<Sensors> {
        let mut system = System::new_all();
        system.refresh_all();
        println!("{:?}", system);
        for component in system.components() {
            println!("{} :{}°C", component.label(), component.temperature());
        }

        let gpu_mem_total = reg::total_gpu_memory()?;

        Ok(Sensors {
            hostname: hostname()?,
            system: Mutex::new(system),
            cpu: Mutex::new(CpuTimeSource::new()?),
            gpu_mem_total,
        })
    }
}

pub fn get_metrics(sensors: &Sensors) -> Result<String> {
    let mut system = sensors.system.lock().unwrap();
    system.refresh_disks();
    system.refresh_networks();
    system.refresh_memory();

    let hostname = &sensors.hostname;
    let mut result = String::with_capacity(256);

    let memory = Memory {
        total: system.total_memory(),
        available: system.available_memory(),
        free: system.free_memory(),
    };
    memory.write(&mut result, &hostname);
    for disk in system.disks() {
        let space = DiskUsage {
            name: disk.name().to_string_lossy().into(),
            size: disk.total_space(),
            free: disk.available_space(),
        };
        space.write(&mut result, &hostname);
    }
    for (interface, net) in system.networks() {
        let usage = NetStats {
            interface: interface.into(),
            bytes_received: net.total_received(),
            bytes_sent: net.total_transmitted(),
        };
        usage.write(&mut result, &hostname);
    }
    let cpu = sensors.cpu.lock().unwrap().read()?;
    cpu.write(&mut result, &hostname);

    let gpu_mem_used = WMI.with(|wmi| wmi.gpu_mem())?;
    let gpu_mem = GpuMemory {
        total: sensors.gpu_mem_total,
        free: sensors.gpu_mem_total - gpu_mem_used,
    };
    gpu_mem.write(&mut result, &hostname);

    let gpu_engines = WMI.with(|wmi| wmi.gpu_usage())?;
    for (name, usage) in gpu_engines.into_iter() {
        let gpu_usage = GpuUsage {
            system: Cow::Owned(name),
            usage,
        };
        gpu_usage.write(&mut result, &hostname);
    }
    if let Some(disk_usage) = WMI.with(|wmi| wmi.disk_usage())? {
        disk_usage.write(&mut result, &hostname);
    }

    Ok(result)
}
