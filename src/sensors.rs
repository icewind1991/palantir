use crate::disk::IoStats;
use color_eyre::{Report, Result};
use std::array::IntoIter;
use std::fs::{read, read_dir, read_to_string, File};
use std::io::{BufRead, BufReader};
use std::os::unix::ffi::OsStrExt;

#[derive(Debug, Clone, Default)]
pub struct Temperatures {
    cpu: f32,
    gpu: f32,
}

impl IntoIterator for Temperatures {
    type Item = (&'static str, f32);
    type IntoIter = IntoIter<Self::Item, 2>;

    fn into_iter(self) -> Self::IntoIter {
        [("cpu", self.cpu), ("gpu", self.gpu)].into_iter()
    }
}

#[derive(Debug, Clone, Default)]
pub struct Memory {
    pub total: u64,
    pub free: u64,
    pub available: u64,
}

pub fn temperatures() -> Result<Temperatures> {
    let mut temps = Temperatures::default();

    const DESIRED_HW_MON: &[&[u8]] = &[b"k10temp\n", b"coretemp\n", b"amdgpu\n"];
    const DESIRED_SENSORS: &[&[u8]] = &[b"Tdie\n", b"edge\n"];

    let mut cores_found = 0.0;
    let mut core_total = 0.0;

    for hwmon in read_dir("/sys/class/hwmon")? {
        let hwmon = hwmon?;
        let hwmon_name = read(hwmon.path().join("name"))?;

        // rpi cpu_thermal doesn't have labels, special case it
        if hwmon_name.as_slice() == b"cpu_thermal\n" {
            let mut path = hwmon.path();
            path.push("temp1_input");
            let value = read_to_string(path)?;
            let parsed: u32 = value.trim().parse()?;
            temps.cpu = parsed as f32 / 1000.0
        }
        if !DESIRED_HW_MON.contains(&hwmon_name.as_slice()) {
            continue;
        }
        for file in read_dir(hwmon.path())? {
            let file = file?;
            let path = file.path();
            let file_name = file.file_name();
            let bytes = file_name.as_bytes();
            let label = if bytes.starts_with(b"temp") && bytes.ends_with(b"_label") {
                read(&path)?
            } else {
                continue;
            };
            if !DESIRED_SENSORS.contains(&label.as_slice()) && !label.starts_with(b"Core") {
                continue;
            }
            let mut path = path
                .into_os_string()
                .into_string()
                .map_err(|_| Report::msg("Invalid hwmon path"))?;
            path.truncate(path.len() - "label".len());
            path.push_str("input");
            let value = read_to_string(path)?;
            let parsed: u32 = value.trim().parse()?;
            match (hwmon_name.as_slice(), label.as_slice()) {
                (b"k10temp\n", b"Tdie\n") => temps.cpu = parsed as f32 / 1000.0,
                (b"amdgpu\n", b"edge\n") => temps.gpu = parsed as f32 / 1000.0,
                (b"coretemp\n", core) if core.starts_with(b"Core") => {
                    cores_found += 1.0;
                    core_total += parsed as f32 / 1000.0
                }
                _ => {}
            }
        }
    }

    if temps.cpu == 0.0 && core_total > 0.0 {
        temps.cpu = core_total / cores_found
    }

    if let Some(nvidia_temperature) = crate::gpu::nvidia::temperature() {
        temps.gpu = nvidia_temperature;
    }

    Ok(temps)
}

pub fn memory() -> Result<Memory> {
    let mut meminfo = BufReader::new(File::open("/proc/meminfo")?);
    let mut mem = Memory::default();
    let mut line = String::new();
    loop {
        line.clear();
        meminfo.read_line(&mut line)?;
        if line.is_empty() {
            break;
        }
        if let Some(line) = line.strip_suffix(" kB\n") {
            if let Some(line_total) = line.strip_prefix("MemTotal: ") {
                mem.total = line_total.trim().parse::<u64>()? * 1000;
            }
            if let Some(line_free) = line.strip_prefix("MemFree: ") {
                mem.free = line_free.trim().parse::<u64>()? * 1000;
            }
            if let Some(line_available) = line.strip_prefix("MemAvailable: ") {
                mem.available = line_available.trim().parse::<u64>()? * 1000;
            }
        }
    }
    Ok(mem)
}

pub fn cpu_time() -> Result<f32> {
    let stat = BufReader::new(File::open("/proc/stat")?);
    let line = stat
        .lines()
        .next()
        .ok_or_else(|| Report::msg("Invalid /proc/stat"))??;
    let mut parts = line.split_ascii_whitespace();
    if let (_cpu, Some(user), _nice, Some(system)) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    {
        let user: f32 = user.parse()?;
        let system: f32 = system.parse()?;
        Ok((user + system) / (clock_ticks()? as f32) / (cpu_count()? as f32))
    } else {
        Err(Report::msg("Invalid /proc/stat"))
    }
}

pub fn network_stats() -> Result<impl Iterator<Item = IoStats>> {
    let stat = BufReader::new(File::open("/proc/net/dev")?);
    Ok(stat
        .lines()
        .filter_map(Result::ok)
        .filter(|line: &String| {
            let trimmed = line.trim_start();
            trimmed.starts_with("en") || trimmed.starts_with("eth")
        })
        .filter_map(|line: String| {
            let mut parts = line.trim_start().split_ascii_whitespace();
            if let (
                Some(interface),
                Some(bytes_received),
                _packets,
                _err,
                _drop,
                _fifo,
                _frame,
                _compressed,
                _multicast,
                Some(bytes_sent),
            ) = (
                parts.next(),
                parts.next(),
                parts.next(),
                parts.next(),
                parts.next(),
                parts.next(),
                parts.next(),
                parts.next(),
                parts.next(),
                parts.next(),
            ) {
                Some(IoStats {
                    interface: interface.trim_end_matches(':').into(),
                    bytes_sent: bytes_sent.parse().ok()?,
                    bytes_received: bytes_received.parse().ok()?,
                })
            } else {
                None
            }
        }))
}

pub fn hostname() -> Result<String> {
    hostname::get()?
        .into_string()
        .map_err(|_| Report::msg("non utf8 hostname"))
}

pub fn clock_ticks() -> Result<u64> {
    let result = unsafe { libc::sysconf(libc::_SC_CLK_TCK) };

    if result > 0 {
        Ok(result as u64)
    } else {
        Err(Report::msg("Failed to get clock ticks"))
    }
}

fn cpu_count() -> Result<u64> {
    let result = unsafe { libc::sysconf(libc::_SC_NPROCESSORS_ONLN) };

    if result < 0 {
        Err(Report::msg("Failed to get cpu count"))
    } else {
        Ok(result as u64)
    }
}
