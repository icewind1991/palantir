pub mod heim;
mod zfs;

use crate::heim::{DiskUsage, Heim, IOStats, Memory, TemperatureLabel};
use crate::zfs::ZFS;
use color_eyre::{Report, Result};
use futures_util::stream::StreamExt;
use futures_util::{pin_mut, try_join};
use std::collections::HashMap;
use std::fmt::Write;
use warp::reject::Reject;
use warp::{Filter, Rejection};

#[derive(Debug)]
struct ReportRejection(Report);

impl From<Report> for ReportRejection {
    fn from(report: Report) -> Self {
        eprintln!("{:#}", report);
        ReportRejection(report)
    }
}

impl Reject for ReportRejection {}

async fn get_metrics(heim: Heim, zfs: ZFS) -> Result<String, ReportRejection> {
    let (hostname, pools, cpu, memory, network, temperatures, disks, disk_usage): (
        String,
        Vec<DiskUsage>,
        f32,
        Memory,
        _,
        HashMap<TemperatureLabel, f32>,
        _,
        _,
    ) = try_join! {
        heim.hostname(),
        zfs.pools(),
        heim.cpu_usage(),
        heim.memory(),
        heim.network_stats(),
        heim.temperatures(),
        heim.disk_stats(),
        heim.disk_usage(),
    }?;
    pin_mut!(network);
    pin_mut!(disks);
    pin_mut!(disk_usage);
    let mut result = String::with_capacity(256);
    writeln!(&mut result, "cpu_usage{{host=\"{}\"}} {:.1}", hostname, cpu).ok();
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
    while let Some(disk) = disk_usage.next().await {
        let disk: DiskUsage = disk;
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
        writeln!(
            &mut result,
            "temperature{{host=\"{}\", sensor=\"{}\"}} {:.1}",
            hostname, label, temp
        )
        .ok();
    }
    Result::<_, ReportRejection>::Ok(result)
}

async fn serve_metrics(heim: Heim, zfs: ZFS) -> Result<String, Rejection> {
    get_metrics(heim, zfs).await.map_err(warp::reject::custom)
}

#[tokio::main]
async fn main() -> Result<()> {
    let host_port: u16 = dotenv::var("PORT")
        .ok()
        .map(|port| port.parse())
        .transpose()?
        .unwrap_or(80);

    ctrlc::set_handler(move || {
        std::process::exit(0);
    })
    .expect("Error setting Ctrl-C handler");

    let heim = warp::any().map(|| Heim::default());
    let zfs = warp::any().map(|| ZFS::default());

    let metrics = warp::path!("metrics")
        .and(heim)
        .and(zfs)
        .and_then(serve_metrics);

    warp::serve(metrics).run(([0, 0, 0, 0], host_port)).await;
    Ok(())
}
