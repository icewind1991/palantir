use color_eyre::{Report, Result};
use once_cell::sync::Lazy;
use parse_display::Display;
use regex::Regex;
use std::collections::HashMap;
use std::ffi::{CStr, CString};
use std::fs::{read, read_dir, read_to_string, DirEntry, File};
use std::io::{BufRead, BufReader};
use std::mem::MaybeUninit;
use std::os::unix::ffi::OsStrExt;

#[derive(Debug, Clone, Hash, Eq, PartialEq, Display)]
#[display(style = "lowercase")]
pub enum TemperatureLabel {
    CPU,
}

#[derive(Debug, Clone, Default)]
pub struct Memory {
    pub total: u64,
    pub free: u64,
    pub available: u64,
}

#[derive(Debug, Clone, Default)]
pub struct IOStats {
    pub interface: String,
    pub bytes_sent: u64,
    pub bytes_received: u64,
}

#[derive(Clone, Debug)]
pub struct DiskUsage {
    pub name: String,
    pub size: u64,
    pub free: u64,
}

pub fn temperatures() -> Result<HashMap<TemperatureLabel, f32>> {
    Ok(read_dir("/sys/class/hwmon")?
        .filter_map(Result::ok)
        .filter_map(|dir: DirEntry| {
            let name = read(dir.path().join("name")).ok()?;
            match name.as_slice() {
                b"k10temp\n" => Some((name, dir)),
                _ => None,
            }
        })
        .flat_map(|(name, dir)| {
            read_dir(dir.path())
                .into_iter()
                .flat_map(|dir| dir)
                .filter_map(Result::ok)
                .filter_map(move |item: DirEntry| {
                    let file_name = item.file_name();
                    let bytes = file_name.as_bytes();
                    if bytes.starts_with(b"temp") && bytes.ends_with(b"_label") {
                        let label = read(item.path()).ok()?;
                        Some((name.clone(), label, item))
                    } else {
                        None
                    }
                })
        })
        .filter_map(
            |(name, label, item)| match (name.as_slice(), label.as_slice()) {
                (b"k10temp\n", b"Tdie\n") => Some((TemperatureLabel::CPU, item)),
                _ => None,
            },
        )
        .filter_map(|(label, item)| {
            let path = item.path().into_os_string();
            Some((label, path.into_string().ok()?))
        })
        .filter_map(|(label, mut path)| {
            path.truncate(path.len() - "label".len());
            path.push_str("input");
            let value = read_to_string(path).ok()?;
            let parsed: u32 = value.trim().parse().ok()?;
            Some((label, parsed as f32 / 1000.0))
        })
        .collect())
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
        if let Some(line) = line.strip_suffix(" kB") {
            if let Some(line_total) = line.strip_prefix("MemTotal: ") {
                mem.total = line_total.trim().parse()?;
            }
            if let Some(line_free) = line.strip_prefix("MemFree: ") {
                mem.free = line_free.trim().parse()?;
            }
            if let Some(line_available) = line.strip_prefix("MemAvailable: ") {
                mem.available = line_available.trim().parse()?;
            }
        }
    }
    Ok(mem)
}

pub fn cpu_time() -> Result<u64> {
    let stat = BufReader::new(File::open("/proc/stat")?);
    let line = stat
        .lines()
        .next()
        .ok_or(Report::msg("Invalid /proc/stat"))??;
    let mut parts = line.split_ascii_whitespace();
    if let (_cpu, Some(user), _nice, Some(system)) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    {
        let user: u64 = user.parse()?;
        let system: u64 = system.parse()?;
        Ok((user + system) * clock_ticks()?)
    } else {
        Err(Report::msg("Invalid /proc/stat"))
    }
}

pub fn network_stats() -> Result<impl Iterator<Item = IOStats>> {
    let stat = BufReader::new(File::open("/proc/net/dev")?);
    Ok(stat
        .lines()
        .filter_map(Result::ok)
        .filter(|line: &String| line.starts_with("enp"))
        .filter_map(|line: String| {
            let mut parts = line.split_ascii_whitespace();
            if let (
                Some(interface),
                Some(bytes_received),
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
            ) {
                Some(IOStats {
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

pub fn disk_stats() -> Result<impl Iterator<Item = IOStats>> {
    static DISK_REGEX: Lazy<Regex> =
        Lazy::new(|| Regex::new(r" ([sv]d[a-z]+|nvme\dn\d) ").unwrap());

    let stat = BufReader::new(File::open("/proc/diskstats")?);
    Ok(stat
        .lines()
        .filter_map(Result::ok)
        .filter(|line| DISK_REGEX.is_match(line))
        .filter_map(|line: String| {
            let mut parts = line.split_whitespace().skip(2);
            let name: String = parts.next()?.into();
            let _read_count = parts.next();
            let _read_merged_count = parts.next();
            let read_bytes = parts.next()?.parse().ok()?;
            let mut parts = parts.skip(1);
            let _write_count = parts.next();
            let _write_merged_count = parts.next();
            let write_bytes = parts.next()?.parse().ok()?;
            Some(IOStats {
                interface: name,
                bytes_sent: write_bytes,
                bytes_received: read_bytes,
            })
        }))
}

pub fn disk_usage() -> Result<impl Iterator<Item = DiskUsage>> {
    let stat = BufReader::new(File::open("/proc/mounts")?);
    Ok(stat
        .lines()
        .filter_map(Result::ok)
        .filter(|line| line.starts_with('/'))
        .filter_map(|line: String| {
            let mount_point = line.split_ascii_whitespace().nth(1)?;
            let mount_point = CString::new(mount_point).ok()?;
            let stat = statvfs(&mount_point).ok()?;
            Some(DiskUsage {
                name: mount_point.into_string().unwrap(),
                size: stat.f_blocks * stat.f_frsize,
                free: stat.f_bavail * stat.f_frsize,
            })
        }))
}

fn clock_ticks() -> Result<u64> {
    let result = unsafe { libc::sysconf(libc::_SC_CLK_TCK) };

    if result > 0 {
        Ok(result as u64)
    } else {
        Err(Report::msg("Failed to get clock ticks"))
    }
}

fn statvfs(path: &CStr) -> Result<libc::statvfs> {
    let mut vfs = MaybeUninit::<libc::statvfs>::uninit();
    let result = unsafe { libc::statvfs(path.as_ptr(), vfs.as_mut_ptr()) };

    if result == 0 {
        let vfs = unsafe { vfs.assume_init() };
        Ok(vfs)
    } else {
        Err(Report::msg("Failed to stat vfs"))
    }
}
