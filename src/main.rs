use bollard::Docker;
use color_eyre::{Report, Result};
use futures_util::pin_mut;
use futures_util::StreamExt;
use libmdns::Responder;
use palantir::disk::zfs::arcstats;
use palantir::docker::{get_docker, stat, Container};
use palantir::get_metrics;
use palantir::gpu::{gpu_metrics, update_gpu_power};
use palantir::power::power_usage;
use std::time::Duration;
use tokio::runtime::Handle;
use tokio::spawn;
use tokio::time::sleep;
use tracing::warn;
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

async fn serve_inner(docker: Option<Docker>) -> Result<String> {
    let mut metrics = get_metrics()?;
    let hostname = palantir::sensors::hostname()?;
    if let Some(docker) = docker {
        let containers = stat(docker).await?;
        pin_mut!(containers);
        while let Some(container) = containers.next().await {
            let container: Container = container;
            container.write(&mut metrics, &hostname);
        }
    }
    if let Some(power) = power_usage()? {
        power.write(&mut metrics, &hostname);
    }
    if let Some(arc) = arcstats()? {
        arc.write(&mut metrics, &hostname);
    }
    gpu_metrics(&mut metrics, &hostname);

    Ok(metrics)
}

async fn serve_metrics(docker: Option<Docker>) -> Result<String, Rejection> {
    serve_inner(docker)
        .await
        .map_err(ReportRejection::from)
        .map_err(warp::reject::custom)
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    let host_port: u16 = dotenvy::var("PORT")
        .ok()
        .map(|port| port.parse())
        .transpose()?
        .unwrap_or(80);

    let mdns = dotenvy::var("DISABLE_MDNS").is_ok();

    ctrlc::set_handler(move || {
        std::process::exit(0);
    })
    .expect("Error setting Ctrl-C handler");

    let docker = get_docker().await;
    let docker = warp::any().map(move || docker.clone());

    if !mdns {
        spawn(setup_mdns(
            hostname::get()?.into_string().unwrap(),
            host_port,
        ));
    }

    std::thread::spawn(update_gpu_power);

    let metrics = warp::path!("metrics").and(docker).and_then(serve_metrics);

    warp::serve(metrics).run(([0, 0, 0, 0], host_port)).await;
    Ok(())
}

async fn setup_mdns(hostname: String, port: u16) {
    let interfaces = if_addrs::get_if_addrs().unwrap_or_default();
    let ip_list: Vec<_> = interfaces
        .into_iter()
        .filter(|interface| !interface.name.contains("docker") && !interface.name.contains("br-"))
        .map(|interface| interface.addr.ip())
        .collect();

    let mdns = loop {
        match Responder::spawn_with_ip_list(&Handle::current(), ip_list.clone()) {
            Ok(mdns) => break mdns,
            Err(e) => {
                warn!(error = display(e), "Failed to register mdns responder");
                sleep(Duration::from_secs(5)).await;
            }
        }
    };

    let _svc = mdns.register(
        "_prometheus-http._tcp".into(),
        hostname,
        port,
        &[&"/metrics"],
    );

    loop {
        sleep(Duration::from_secs(60 * 60)).await;
    }
}
