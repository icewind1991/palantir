use crate::data::{DiskStats, DiskUsage};
use crate::{Error, MultiSensorSource, Result};
use ahash::{AHashSet, AHasher};
use regex::Regex;
use std::ffi::CString;
use std::fs::File;
use std::hash::{Hash, Hasher};
use std::io::{Read, Seek};
use std::mem::MaybeUninit;
use tracing::{debug, error};

pub mod zfs;

pub struct DiskStatSource {
    source: File,
    buff: String,
    regex: Regex,
}

impl DiskStatSource {
    pub fn new() -> Result<DiskStatSource> {
        Ok(DiskStatSource {
            source: File::open("/proc/diskstats")?,
            buff: String::new(),
            regex: Regex::new(r" ([sv]d[a-z]+|nvme[0-9]n[0-9]|mmcblk[0-9]) ").unwrap(),
        })
    }
}

impl MultiSensorSource for DiskStatSource {
    type Data = DiskStats;
    type Iter<'a> = DiskStatParser<'a>;

    fn read(&mut self) -> Result<Self::Iter<'_>> {
        self.buff.clear();
        self.source.rewind()?;
        self.source.read_to_string(&mut self.buff)?;

        Ok(DiskStatParser {
            lines: self.buff.lines(),
            regex: &self.regex,
        })
    }
}

pub struct DiskStatParser<'a> {
    lines: std::str::Lines<'a>,
    regex: &'a Regex,
}

impl Iterator for DiskStatParser<'_> {
    type Item = Result<DiskStats>;

    fn next(&mut self) -> Option<Self::Item> {
        let line = loop {
            let line = self.lines.next()?;
            if self.regex.is_match(line) {
                break line;
            }
        };
        let mut parts = line.split_whitespace().skip(2);
        let name: String = parts.next()?.into();
        let _read_count = parts.next();
        let _read_merged_count = parts.next();
        let read_sectors = parts.next()?.parse::<u64>().ok()?;
        let mut parts = parts.skip(1);
        let _write_count = parts.next();
        let _write_merged_count = parts.next();
        let write_sectors = parts.next()?.parse::<u64>().ok()?;
        Some(Ok(DiskStats {
            interface: name,
            bytes_sent: write_sectors * 512,
            bytes_received: read_sectors * 512,
        }))
    }
}

pub struct DiskUsageSource {
    source: File,
    buff: String,
}

impl DiskUsageSource {
    pub fn new() -> Result<DiskUsageSource> {
        Ok(DiskUsageSource {
            source: File::open("/proc/mounts")?,
            buff: String::new(),
        })
    }
}

impl MultiSensorSource for DiskUsageSource {
    type Data = DiskUsage;
    type Iter<'a> = DiskUsageParser<'a>;

    fn read(&mut self) -> Result<Self::Iter<'_>> {
        self.buff.clear();
        self.source.rewind()?;
        self.source.read_to_string(&mut self.buff)?;

        Ok(DiskUsageParser {
            lines: self.buff.lines(),
            found_disks: AHashSet::with_capacity(16),
        })
    }
}

pub struct DiskUsageParser<'a> {
    lines: std::str::Lines<'a>,
    found_disks: AHashSet<u64>,
}

impl Iterator for DiskUsageParser<'_> {
    type Item = Result<DiskUsage>;

    fn next(&mut self) -> Option<Self::Item> {
        let mount_point = loop {
            let line = self.lines.next()?;
            if line.starts_with('/') && !line.contains("/dev/loop") && !line.contains("fuse") {
                debug!(line, "picking mount");

                let mut parts = line.split_ascii_whitespace();
                let disk = parts.next()?;
                if self.found_disks.insert(hash_str(disk)) {
                    let mount_point = parts.next()?;

                    break mount_point;
                } else {
                    debug!(line, "skipping already processed disk");
                }
            } else {
                debug!(line, "skipping mount");
            }
        };

        let stat = match statvfs(mount_point) {
            Ok(stat) => stat,
            Err(e) => {
                error!(error = ?e, "error while getting disk statistics");
                return Some(Err(e));
            }
        };
        // cast is needed on 32bit platforms
        #[allow(clippy::unnecessary_cast)]
        Some(Ok(DiskUsage {
            name: mount_point.to_string(),
            size: stat.f_blocks * stat.f_frsize as u64,
            free: stat.f_bavail * stat.f_frsize as u64,
        }))
    }
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
