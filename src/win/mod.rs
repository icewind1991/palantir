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
use std::thread::spawn;
use sysinfo::{Components, Disks, Networks, System};

pub struct Sensors {
    pub hostname: String,
    pub system: Mutex<System>,
    pub networks: Mutex<Networks>,
    pub components: Mutex<Components>,
    pub disks: Mutex<Disks>,
    cpu: Mutex<CpuTimeSource>,
    gpu_mem_total: u64,
}

static WMI: Lazy<ThreadLocal<WmiSensor>> =
    Lazy::new(|| ThreadLocal::new(|| WmiSensor::new().expect("failed to init wmi")));

impl Sensors {
    pub fn new() -> Result<Sensors> {
        spawn(wmi::update_power);
        let gpu_mem_total = reg::total_gpu_memory()?;

        Ok(Sensors {
            hostname: hostname()?,
            system: Mutex::new(System::new()),
            networks: Mutex::new(Networks::new_with_refreshed_list()),
            components: Mutex::new(Components::new_with_refreshed_list()),
            disks: Mutex::new(Disks::new_with_refreshed_list()),
            cpu: Mutex::new(CpuTimeSource::new()?),
            gpu_mem_total,
        })
    }
}

pub fn get_metrics(sensors: &Sensors) -> Result<String> {
    let mut system = sensors.system.lock().unwrap();
    let mut networks = sensors.networks.lock().unwrap();
    let mut components = sensors.components.lock().unwrap();
    let mut disks = sensors.disks.lock().unwrap();

    system.refresh_all();
    networks.refresh();
    components.refresh();
    disks.refresh();

    let hostname = &sensors.hostname;
    let mut result = String::with_capacity(256);

    let memory = Memory {
        total: system.total_memory(),
        available: system.available_memory(),
        free: system.free_memory(),
    };
    memory.write(&mut result, hostname);
    for disk in disks.iter() {
        let space = DiskUsage {
            name: disk.name().to_string_lossy().into(),
            size: disk.total_space(),
            free: disk.available_space(),
        };
        space.write(&mut result, hostname);
    }
    for (interface, net) in networks.iter() {
        let usage = NetStats {
            interface: interface.into(),
            bytes_received: net.total_received(),
            bytes_sent: net.total_transmitted(),
        };
        usage.write(&mut result, hostname);
    }
    let cpu = sensors.cpu.lock().unwrap().read()?;
    cpu.write(&mut result, hostname);

    let gpu_mem_used = WMI.with(|wmi| wmi.gpu_mem())?;
    let gpu_mem = GpuMemory {
        total: sensors.gpu_mem_total,
        free: sensors.gpu_mem_total - gpu_mem_used,
    };
    gpu_mem.write(&mut result, hostname);

    let gpu_engines = WMI.with(|wmi| wmi.gpu_usage())?;
    for (name, usage) in gpu_engines.into_iter() {
        let gpu_usage = GpuUsage {
            system: Cow::Owned(name),
            usage,
        };
        gpu_usage.write(&mut result, hostname);
    }
    if let Some(disk_usage) = WMI.with(|wmi| wmi.disk_usage())? {
        disk_usage.write(&mut result, hostname);
    }
    let hwmon_data = WMI.with(|wmi| wmi.hwmon())?;
    hwmon_data.temperature.write(&mut result, hostname);
    hwmon_data.cpu_power.write(&mut result, hostname);
    hwmon_data.gpu_power.write(&mut result, hostname);

    Ok(result)
}
