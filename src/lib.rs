pub mod sensors;
pub mod zfs;

use crate::sensors::temperatures;
use crate::sensors::*;
use crate::zfs::pools;
use color_eyre::Result;
use futures_util::stream::StreamExt;
use futures_util::{pin_mut, try_join};
use std::collections::HashSet;
use std::fmt::Write;

pub async fn get_metrics() -> Result<String> {
    let (hostname, cpu, network, disks, disk_usage) = try_join! {
        hostname(),
        cpu_time(),
        network_stats(),
        disk_stats(),
        disk_usage(),
    }?;
    let memory = memory()?;
    let temperatures = temperatures()?;
    let pools = pools();
    pin_mut!(network);
    pin_mut!(disks);
    pin_mut!(disk_usage);
    let mut result = String::with_capacity(256);
    writeln!(&mut result, "cpu_time{{host=\"{}\"}} {:.1}", hostname, cpu).ok();
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
    while let Some(network) = network.next().await {
        let network: IOStats = network;
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
    while let Some(disk) = disks.next().await {
        let disk: IOStats = disk;
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

    let mut found_sizes = HashSet::new();
    while let Some(disk) = disk_usage.next().await {
        let disk: DiskUsage = disk;
        if disk.size > 0 {
            if found_sizes.insert((disk.size, disk.free)) {
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
    }
    for (label, temp) in temperatures {
        writeln!(
            &mut result,
            "temperature{{host=\"{}\", sensor=\"{}\"}} {:.1}",
            hostname, label, temp
        )
        .ok();
    }
    Ok(result)
}
