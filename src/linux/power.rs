use crate::data::{CpuPowerUsage, GpuPowerUsage};
use crate::linux::gpu::gpu_power;
use crate::linux::hwmon::FileSource;
use crate::{IoResultExt, Result, SensorSource};
use std::fs::read_dir;

#[derive(Default)]
pub struct CpuPowerSource {
    sources: Vec<FileSource>,
}

impl CpuPowerSource {
    pub fn new() -> Result<CpuPowerSource> {
        let sources: Vec<_> = read_dir("/sys/devices/virtual/powercap/intel-rapl")
            .context("error listing power devices")?
            .flatten()
            .filter(|path| {
                path.file_name()
                    .to_str()
                    .unwrap_or_default()
                    .starts_with("intel-rapl")
            })
            .map(|entry| {
                let mut path = entry.path();
                path.push("energy_uj");
                path
            })
            .flat_map(FileSource::open)
            .collect();

        Ok(CpuPowerSource { sources })
    }
}

impl SensorSource for CpuPowerSource {
    type Data = CpuPowerUsage;

    fn read(&mut self) -> Result<Self::Data> {
        let mut usage = CpuPowerUsage::default();
        for source in self.sources.iter_mut() {
            let package_usage = source.read().context("error reading power source")?;
            usage.cpu_uj += package_usage;
            usage.cpu_packages_uj.push(package_usage);
        }
        Ok(usage)
    }
}

#[derive(Default)]
pub struct GpuPowerSource;

impl SensorSource for GpuPowerSource {
    type Data = GpuPowerUsage;

    fn read(&mut self) -> Result<Self::Data> {
        let gpu_uj = crate::linux::gpu::nvidia::power().unwrap_or_else(gpu_power);
        Ok(GpuPowerUsage { gpu_uj })
    }
}
