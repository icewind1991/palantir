use crate::data::PowerUsage;
use crate::linux::gpu::gpu_power;
use crate::{Error, Result};
use std::fmt::Write;
use std::fs::{read_dir, read_to_string};
use std::sync::atomic::{AtomicBool, Ordering};
use tracing::warn;

static CAN_READ: AtomicBool = AtomicBool::new(true);

pub fn power_usage() -> Result<Option<PowerUsage>> {
    if !CAN_READ.load(Ordering::Relaxed) {
        return Ok(None);
    }

    let dir = match read_dir("/sys/devices/virtual/powercap/intel-rapl") {
        Ok(dir) => dir,
        Err(_) => {
            CAN_READ.store(false, Ordering::Relaxed);
            return Ok(None);
        }
    };
    let mut usage = PowerUsage::default();
    for package in dir {
        let package = package?;
        if package
            .file_name()
            .to_str()
            .ok_or_else(|| Error::Other("Invalid name".into()))?
            .starts_with("intel-rapl")
        {
            let mut package_path = package.path();
            package_path.push("energy_uj");
            let package_usage = match read_to_string(&package_path) {
                Err(e) if e.raw_os_error() == Some(13) => {
                    CAN_READ.store(false, Ordering::Relaxed);
                    warn!(
                        package_path = display(package_path.display()),
                        "can\'t read power usage"
                    );
                    return Ok(None);
                }
                result => result,
            }?;
            let package_usage = package_usage.trim().parse::<u64>()?;
            usage.cpu_uj += package_usage;
            usage.cpu_packages_uj.push(package_usage);
        }
    }

    usage.gpu_uj = gpu_power();
    if let Some(nvidia_power) = crate::linux::gpu::nvidia::power() {
        usage.gpu_uj = nvidia_power;
    }

    Ok(Some(usage))
}
