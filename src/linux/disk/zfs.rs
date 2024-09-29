use crate::linux::disk::DiskUsage;
use crate::{IoResultExt, Result};
use std::fmt::Write;
use std::fs::read_to_string;
use std::process::Command;
use std::str::FromStr;
use std::sync::atomic::{AtomicBool, Ordering};
use tracing::warn;

static CAN_READ: AtomicBool = AtomicBool::new(true);

pub fn pools() -> impl Iterator<Item = DiskUsage> {
    if !CAN_READ.load(Ordering::Relaxed) {
        return ZPoolOutputParser::default();
    }

    ZPoolOutputParser {
        str: zpool_command().unwrap_or_default(),
        pos: 0,
    }
}

fn zpool_command() -> Result<String> {
    let mut z = Command::new("zpool");
    z.args(["list", "-p", "-H", "-o", "name,size,free"]);
    let out = z.output().context("error getting zpool list")?;
    if out.status.success() {
        Ok(String::from_utf8(out.stdout)?)
    } else {
        CAN_READ.store(false, Ordering::Relaxed);
        warn!(
            status = out.status.code().unwrap_or(-1),
            stdout = String::from_utf8(out.stdout).unwrap_or_else(|_| String::from("non utf8")),
            stderr = String::from_utf8(out.stderr).unwrap_or_else(|_| String::from("non utf8")),
            "Failed to list zpool status"
        );
        Ok(String::new())
    }
}

fn parse_line(line: &str) -> Option<DiskUsage> {
    let mut parts = line.split_ascii_whitespace();
    let name = parts.next()?.to_string();
    let size = parts.next()?.parse().ok()?;
    let free = parts.next()?.parse().ok()?;
    Some(DiskUsage { name, size, free })
}

#[derive(Default)]
struct ZPoolOutputParser {
    str: String,
    pos: usize,
}

impl Iterator for ZPoolOutputParser {
    type Item = DiskUsage;

    fn next(&mut self) -> Option<Self::Item> {
        let str = self.str.as_str();
        let line = match str[self.pos..].find('\n') {
            Some(next_pos) => {
                let old_pos = self.pos;
                self.pos += next_pos + 1;
                Some(&str[old_pos..self.pos])
            }
            None if self.pos < str.len() => {
                let old_pos = self.pos;
                self.pos = str.len();
                Some(&str[old_pos..])
            }
            None => None,
        };
        line.and_then(parse_line)
    }
}

#[derive(Debug, Default)]
pub struct ArcStats {
    hits: u64,
    misses: u64,
    prefetch: u64,
    size: u64,
}

impl ArcStats {
    pub fn write<W: Write>(&self, mut w: W, hostname: &str) {
        writeln!(
            &mut w,
            "zfs_arc_hits{{host=\"{}\"}} {}",
            hostname, self.hits
        )
        .ok();
        writeln!(
            &mut w,
            "zfs_arc_misses{{host=\"{}\"}} {}",
            hostname, self.misses
        )
        .ok();
        writeln!(
            &mut w,
            "zfs_arc_size{{host=\"{}\"}} {}",
            hostname, self.size
        )
        .ok();
        writeln!(
            &mut w,
            "zfs_arc_prefetch{{host=\"{}\"}} {}",
            hostname, self.prefetch
        )
        .ok();
    }
}

pub fn arcstats() -> Option<ArcStats> {
    let content = match read_to_string("/proc/spl/kstat/zfs/arcstats") {
        Ok(c) => c,
        Err(_) => return None,
    };
    let mut stats = ArcStats::default();

    for line in content.lines().skip(2) {
        let mut parts = line.split_ascii_whitespace();
        if let (Some(name), _, Some(Ok(value))) =
            (parts.next(), parts.next(), parts.next().map(u64::from_str))
        {
            match name {
                "demand_data_hits" => stats.hits += value,
                "demand_metadata_hits" => stats.hits += value,
                "prefetch_data_hits" => {
                    stats.hits += value;
                    stats.prefetch += value;
                }
                "prefetch_metadata_hits" => {
                    stats.hits += value;
                    stats.prefetch += value;
                }
                "demand_data_misses" => stats.misses += value,
                "demand_metadata_misses" => stats.misses += value,
                "prefetch_data_misses" => {
                    stats.misses += value;
                    stats.prefetch += value;
                }
                "prefetch_metadata_misses" => {
                    stats.misses += value;
                    stats.prefetch += value;
                }
                "size" => stats.size = value,
                _ => {}
            }
        }
    }

    Some(stats)
}
