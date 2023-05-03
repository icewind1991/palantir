use crate::{Error, Result, SensorData};
use ahash::{AHashSet, AHasher};
use once_cell::sync::Lazy;
use regex::Regex;
use std::ffi::CString;
use std::fmt::Write;
use std::fs::File;
use std::hash::{Hash, Hasher};
use std::io::{BufRead, BufReader};
use std::mem::MaybeUninit;

pub mod zfs;

#[derive(Debug, Clone, Default)]
pub struct IoStats {
    pub interface: String,
    pub bytes_sent: u64,
    pub bytes_received: u64,
}

impl SensorData for IoStats {
    fn write<W: Write>(&self, mut w: W, hostname: &str) {
        if self.bytes_received > 0 || self.bytes_sent > 0 {
            writeln!(
                &mut w,
                "net_sent{{host=\"{}\", network=\"{}\"}} {}",
                hostname, self.interface, self.bytes_sent
            )
            .ok();
            writeln!(
                &mut w,
                "net_received{{host=\"{}\", network=\"{}\"}} {}",
                hostname, self.interface, self.bytes_received
            )
            .ok();
        }
    }
}

#[derive(Clone, Debug)]
pub struct DiskUsage {
    pub name: String,
    pub size: u64,
    pub free: u64,
}

pub fn disk_stats() -> Result<impl Iterator<Item = IoStats>> {
    static DISK_REGEX: Lazy<Regex> =
        Lazy::new(|| Regex::new(r" ([sv]d[a-z]+|nvme[0-9]n[0-9]|mmcblk[0-9]) ").unwrap());

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
            let read_sectors = parts.next()?.parse::<u64>().ok()?;
            let mut parts = parts.skip(1);
            let _write_count = parts.next();
            let _write_merged_count = parts.next();
            let write_sectors = parts.next()?.parse::<u64>().ok()?;
            Some(IoStats {
                interface: name,
                bytes_sent: write_sectors * 512,
                bytes_received: read_sectors * 512,
            })
        }))
}

pub fn disk_usage() -> Result<impl Iterator<Item = DiskUsage>> {
    let stat = BufReader::new(File::open("/proc/mounts")?);
    let mut found_disks = AHashSet::with_capacity(8);
    Ok(stat
        .lines()
        .filter_map(Result::ok)
        .filter(|line| line.starts_with('/'))
        .filter(|line| !line.contains("/dev/loop"))
        .filter(|line| !line.contains("fuse"))
        .filter_map(move |line: String| {
            let mut parts = line.split_ascii_whitespace();
            let disk = parts.next()?;
            if !found_disks.insert(hash_str(disk)) {
                return None;
            }
            let mount_point = parts.next()?;
            let stat = statvfs(&mount_point).ok()?;
            Some(DiskUsage {
                name: mount_point.to_string(),
                size: stat.f_blocks * stat.f_frsize as u64,
                free: stat.f_bavail * stat.f_frsize as u64,
            })
        }))
}

fn statvfs(path: &str) -> Result<libc::statvfs> {
    let path = CString::new(path)?;
    let mut vfs = MaybeUninit::<libc::statvfs>::uninit();
    let result = unsafe { libc::statvfs(path.as_ptr(), vfs.as_mut_ptr()) };

    if result == 0 {
        let vfs = unsafe { vfs.assume_init() };
        Ok(vfs)
    } else {
        Err(Error::StatVfs)
    }
}

fn hash_str(s: &str) -> u64 {
    let mut hasher = AHasher::default();
    s.hash(&mut hasher);
    hasher.finish()
}
