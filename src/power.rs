use color_eyre::{Report, Result};
use std::fmt::Write;
use std::fs::{read_dir, read_to_string};
use std::sync::atomic::{AtomicBool, Ordering};

static CAN_READ: AtomicBool = AtomicBool::new(true);

#[derive(Debug, Default)]
pub struct PowerUsage {
    total_uj: u64,
    packages_uj: Vec<u64>,
}

impl PowerUsage {
    pub fn write<W: Write>(&self, mut w: W, hostname: &str) {
        writeln!(
            &mut w,
            "total_power{{host=\"{}\"}} {:.3}",
            hostname,
            self.total_uj as f64 / 1_000_000.0
        )
        .ok();
        for (i, package) in self.packages_uj.iter().enumerate() {
            writeln!(
                &mut w,
                "total_power{{host=\"{}\", package=\"{}\"}} {:.3}",
                hostname,
                i,
                *package as f64 / 1_000_000.0
            )
            .ok();
        }
    }
}
pub fn power_usage() -> Result<Option<PowerUsage>> {
    if !CAN_READ.load(Ordering::Relaxed) {
        return Ok(None);
    }

    let dir = read_dir("/sys/devices/virtual/powercap/intel-rapl")?;
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
                    return Ok(None);
                }
                result => result,
            }?;
            let package_usage = dbg!(package_usage.trim().parse::<u64>()?);
            usage.total_uj += package_usage;
            usage.packages_uj.push(package_usage);
        }
    }
    Ok(Some(usage))
}
