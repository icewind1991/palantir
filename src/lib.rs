pub mod disk;
pub mod docker;
pub mod gpu;
pub mod power;
pub mod sensors;

use crate::disk::disk_usage;
use crate::disk::zfs::pools;
use crate::disk::*;
use crate::sensors::*;
use color_eyre::Result;
use std::fmt::Write;

pub fn get_metrics() -> Result<String> {
    let disk_usage = disk_usage()?;
    let disks = disk_stats()?;
    let cpu = cpu_time()?;
    let hostname = hostname()?;
    let memory = memory()?;
    let temperatures = temperatures()?;
    let pools = pools();
    let networks = network_stats()?;
    let mut result = String::with_capacity(256);
    writeln!(&mut result, "cpu_time{{host=\"{}\"}} {:.3}", hostname, cpu).ok();
    writeln!(
        &mut result,
        "memory_total{{host=\"{}\"}} {}",
        hostname, memory.total
    )
    .ok();
    writeln!(
        &mut result,
        "memory_available{{host=\"{}\"}} {}",
        hostname, memory.available
    )
    .ok();
    writeln!(
        &mut result,
        "memory_free{{host=\"{}\"}} {}",
        hostname, memory.free
    )
    .ok();
    for pool in pools {
        writeln!(
            &mut result,
            "zfs_pool_size{{host=\"{}\", pool=\"{}\"}} {}",
            hostname, pool.name, pool.size
        )
        .ok();
        writeln!(
            &mut result,
            "zfs_pool_free{{host=\"{}\", pool=\"{}\"}} {}",
            hostname, pool.name, pool.free
        )
        .ok();
    }
    for network in networks {
        if network.bytes_received > 0 || network.bytes_sent > 0 {
            writeln!(
                &mut result,
                "net_sent{{host=\"{}\", network=\"{}\"}} {}",
                hostname, network.interface, network.bytes_sent
            )
            .ok();
            writeln!(
                &mut result,
                "net_received{{host=\"{}\", network=\"{}\"}} {}",
                hostname, network.interface, network.bytes_received
            )
            .ok();
        }
    }
    for disk in disks {
        if disk.bytes_received > 0 && disk.bytes_sent > 0 {
            writeln!(
                &mut result,
                "disk_sent{{host=\"{}\", disk=\"{}\"}} {}",
                hostname, disk.interface, disk.bytes_sent
            )
            .ok();
            writeln!(
                &mut result,
                "disk_received{{host=\"{}\", disk=\"{}\"}} {}",
                hostname, disk.interface, disk.bytes_received
            )
            .ok();
        }
    }

    for disk in disk_usage {
        if disk.size > 0 {
            writeln!(
                &mut result,
                "disk_size{{host=\"{}\", disk=\"{}\"}} {}",
                hostname, disk.name, disk.size
            )
            .ok();
            writeln!(
                &mut result,
                "disk_free{{host=\"{}\", disk=\"{}\"}} {}",
                hostname, disk.name, disk.free
            )
            .ok();
        }
    }
    for (label, temp) in temperatures {
        if temp != 0.0 {
            writeln!(
                &mut result,
                "temperature{{host=\"{}\", sensor=\"{}\"}} {:.1}",
                hostname, label, temp
            )
            .ok();
        }
    }
    Ok(result)
}
