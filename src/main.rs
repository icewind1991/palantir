mod heim;
mod zfs;

use crate::heim::{Heim, Memory, NetworkStats, TemperatureLabel};
use crate::zfs::{ZfsPool, ZFS};
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
    let (hostname, pools, cpu, memory, network, temperatures): (
        String,
        Vec<ZfsPool>,
        f32,
        Memory,
        _,
        HashMap<TemperatureLabel, f32>,
    ) = try_join! {
        heim.hostname(),
        zfs.pools(),
        heim.cpu_usage(),
        heim.memory(),
        heim.network_stats(),
        heim.temperatures(),
    }?;
    pin_mut!(network);
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
        if network.bytes_received > 0 || network.bytes_sent > 0 {
            let network: NetworkStats = network;
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
