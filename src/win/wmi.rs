use crate::data::{CpuPowerUsage, DiskStats, GpuPowerUsage, Temperatures};
use crate::Result;
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::thread::sleep;
use std::time::{Duration, Instant};
use wmi::{COMLibrary, WMIConnection};

pub struct WmiSensor {
    wmi_con: WMIConnection,
    wmi_hwmon_con: Option<WMIConnection>,
}

impl WmiSensor {
    pub fn new() -> Result<Self> {
        let com_con = COMLibrary::new()?;
        let wmi_con = WMIConnection::new(com_con)?;
        let wmi_hwmon_con =
            WMIConnection::with_namespace_path("ROOT\\LibreHardwareMonitor", com_con).ok();

        Ok(WmiSensor {
            wmi_con,
            wmi_hwmon_con,
        })
    }

    pub fn gpu_mem(&self) -> Result<u64> {
        #[derive(Deserialize, Debug)]
        #[serde(rename = "Win32_PerfFormattedData_GPUPerformanceCounters_GPUAdapterMemory")]
        struct GPUAdapterMemory {
            #[serde(rename = "DedicatedUsage")]
            dedicated_usage: u64,
        }

        let results: Vec<GPUAdapterMemory> = self.wmi_con.query()?;
        Ok(results.iter().map(|result| result.dedicated_usage).sum())
    }

    pub fn gpu_usage(&self) -> Result<HashMap<String, u32>> {
        #[derive(Deserialize, Debug)]
        #[serde(rename = "Win32_PerfFormattedData_GPUPerformanceCounters_GPUEngine")]
        struct GPUEngine {
            #[serde(rename = "Name")]
            name: String,
            #[serde(rename = "UtilizationPercentage")]
            usage: u32,
        }

        let results: Vec<GPUEngine> = self.wmi_con.query()?;

        let mut data = HashMap::default();

        for result in results {
            if let Some(eng_type) = result.name.split("_engtype_").nth(1) {
                let entry = data.entry(eng_type.to_string()).or_default();
                *entry += result.usage;
            }
        }

        Ok(data)
    }

    pub fn disk_usage(&self) -> Result<Option<DiskStats>> {
        #[derive(Deserialize, Debug)]
        #[serde(rename = "Win32_PerfRawData_Counters_FileSystemDiskActivity")]
        struct FileSystemDiskActivity {
            #[serde(rename = "Name")]
            name: String,
            #[serde(rename = "FileSystemBytesRead")]
            read: u64,
            #[serde(rename = "FileSystemBytesWritten")]
            written: u64,
        }

        let results: Vec<FileSystemDiskActivity> = self.wmi_con.query()?;
        for result in results {
            if result.name == "_Total" {
                return Ok(Some(DiskStats {
                    interface: "Total".to_string(),
                    bytes_sent: result.written,
                    bytes_received: result.read,
                }));
            }
        }
        Ok(None)
    }

    pub fn hwmon(&self) -> Result<HwMonData> {
        let sensors: Vec<Sensor> = match self.wmi_hwmon_con.as_ref() {
            Some(wmi) => wmi.query()?,
            None => Vec::default(),
        };

        let temperature = Temperatures {
            cpu: avg_sensors(&sensors, |sensor| {
                sensor.sensor_type == "Temperature"
                    && sensor.name.starts_with("CPU Core")
                    && !sensor.name.contains("Distance")
            }),
            gpu: avg_sensors(&sensors, |sensor| {
                sensor.sensor_type == "Temperature" && sensor.name == "GPU Core"
            }),
        };
        Ok(HwMonData {
            temperature,
            cpu_power: cpu_power(),
            gpu_power: gpu_power(),
        })
    }
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "PascalCase")]
#[allow(dead_code)]
struct Sensor {
    identifier: String,
    name: String,
    sensor_type: String,
    value: f32,
}

fn avg_sensors(sensors: &[Sensor], filter: impl Fn(&Sensor) -> bool) -> f32 {
    let count = sensors.iter().filter(|sensor| filter(sensor)).count();
    let total: f32 = sensors
        .iter()
        .filter_map(|sensor| filter(sensor).then_some(sensor.value))
        .sum();
    total / count as f32
}

pub struct HwMonData {
    pub temperature: Temperatures,
    pub cpu_power: CpuPowerUsage,
    pub gpu_power: GpuPowerUsage,
}

static CPU_POWER_UJ: AtomicU64 = AtomicU64::new(0);
static GPU_POWER_UJ: AtomicU64 = AtomicU64::new(0);
static POWER_LAST_READ: Mutex<Option<Instant>> = Mutex::new(None);

fn get_power_elapsed() -> Option<Duration> {
    let mut last_read = POWER_LAST_READ.lock().unwrap();
    let now = Instant::now();
    let elapsed = last_read.as_ref().map(|last_read| now - *last_read);
    *last_read = Some(now);
    elapsed
}

fn get_sensor(sensors: &[Sensor], ty: &str, name: &str) -> Option<f32> {
    sensors.iter().find_map(|sensor| {
        (sensor.sensor_type == ty && sensor.name == name).then_some(sensor.value)
    })
}

pub fn update_power() {
    let Ok(com_con) = COMLibrary::new() else {return;};
    if let Ok(wmi_con) = WMIConnection::with_namespace_path("ROOT\\LibreHardwareMonitor", com_con) {
        loop {
            if let Some(elapsed) = get_power_elapsed() {
                let Ok(sensors) = wmi_con.query::<Sensor>() else {return;};
                let sensors: Vec<Sensor> = sensors;
                let Some(cpu_current_power) = get_sensor(&sensors, "Power", "CPU Package") else {return;};
                let Some(gpu_current_power) = get_sensor(&sensors, "Power", "GPU Package") else {return;};

                let elapsed_sec = elapsed.as_secs_f32();

                let cpu_power = cpu_current_power * elapsed_sec * 1_000_000.0;
                let gpu_power = gpu_current_power * elapsed_sec * 1_000_000.0;

                CPU_POWER_UJ.fetch_add(cpu_power as u64, Ordering::SeqCst);
                GPU_POWER_UJ.fetch_add(gpu_power as u64, Ordering::SeqCst);
            }
            sleep(Duration::from_millis(500));
        }
    }
}

pub fn cpu_power() -> CpuPowerUsage {
    CpuPowerUsage {
        cpu_uj: CPU_POWER_UJ.load(Ordering::SeqCst),
        cpu_packages_uj: Vec::default(),
    }
}

pub fn gpu_power() -> GpuPowerUsage {
    GpuPowerUsage {
        gpu_uj: GPU_POWER_UJ.load(Ordering::SeqCst),
    }
}
