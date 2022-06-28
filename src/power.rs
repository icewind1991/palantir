use color_eyre::{Report, Result};
use std::fmt::Write;
use std::fs::{read_dir, read_to_string};
use std::sync::atomic::{AtomicBool, Ordering};
use tracing::warn;

static CAN_READ: AtomicBool = AtomicBool::new(true);

#[derive(Debug, Default)]
pub struct PowerUsage {
    cpu_uj: u64,
    cpu_packages_uj: Vec<u64>,
    gpu_uj: u64,
}

impl PowerUsage {
    pub fn write<W: Write>(&self, mut w: W, hostname: &str) {
        writeln!(
            &mut w,
            r#"total_power{{host="{}", device="cpu"}} {:.3}"#,
            hostname,
            self.cpu_uj as f64 / 1_000_000.0
        )
        .ok();
        for (i, package) in self.cpu_packages_uj.iter().enumerate() {
            writeln!(
                &mut w,
                r#"package_power{{host="{}", package="{}", device="cpu"}} {:.3}"#,
                hostname,
                i,
                *package as f64 / 1_000_000.0
            )
            .ok();
        }
        if self.gpu_uj > 0 {
            writeln!(
                &mut w,
                r#"total_power{{host="{}", device="gpu"}} {:.3}"#,
                hostname,
                self.gpu_uj as f64 / 1_000_000.0
            )
            .ok();
        }
    }
}

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
            .ok_or_else(|| Report::msg("Invalid name"))?
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

    if let Some(nvidia_power) = crate::nvidia::power() {
        usage.gpu_uj = nvidia_power;
    }

    Ok(Some(usage))
}
