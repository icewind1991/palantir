use bollard::Docker;
use clap::Parser;
use color_eyre::{Report, Result};
use futures_util::pin_mut;
use futures_util::StreamExt;
use libmdns::Responder;
use palantir::docker::{get_docker, stat, Container};
use palantir::{get_metrics, Sensors};
use std::sync::Arc;
use std::time::Duration;
use tokio::runtime::Handle;
use tokio::spawn;
use tokio::time::sleep;
use tracing::warn;
use warp::reject::Reject;
use warp::{Filter, Rejection};

#[derive(Debug)]
#[allow(dead_code)]
struct ReportRejection(Report);

impl From<Report> for ReportRejection {
    fn from(report: Report) -> Self {
        eprintln!("{:#}", report);
        ReportRejection(report)
    }
}

impl Reject for ReportRejection {}

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Port to listen to
    #[arg(short, long)]
    port: Option<u16>,
}

async fn serve_inner(docker: Option<Docker>, sensors: &Sensors) -> Result<String> {
    let mut metrics = get_metrics(sensors)?;
    if let Some(docker) = docker {
        let containers = stat(docker).await?;
        pin_mut!(containers);
        while let Some(container) = containers.next().await {
            let container: Container = container;
            container.write(&mut metrics, &sensors.hostname);
        }
    }

    Ok(metrics)
}

async fn serve_metrics(docker: Option<Docker>, sensors: Arc<Sensors>) -> Result<String, Rejection> {
    serve_inner(docker, &sensors)
        .await
        .map_err(ReportRejection::from)
        .map_err(warp::reject::custom)
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    let args = Args::parse();

    let host_port = match args.port {
        Some(port) => port,
        None => dotenvy::var("PORT")
            .ok()
            .map(|port| port.parse())
            .transpose()?
            .unwrap_or(80),
    };

    let mdns = dotenvy::var("DISABLE_MDNS").is_ok();

    ctrlc::set_handler(move || {
        std::process::exit(0);
    })
    .expect("Error setting Ctrl-C handler");

    let docker = get_docker().await;
    let docker = warp::any().map(move || docker.clone());
    let sensors = Arc::new(Sensors::new()?);
    let sensors = warp::any().map(move || sensors.clone());

    if !mdns {
        spawn(setup_mdns(
            hostname::get()?.into_string().unwrap(),
            host_port,
        ));
    }

    let metrics = warp::path!("metrics")
        .and(docker)
        .and(sensors)
        .and_then(serve_metrics);

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
        &["/metrics"],
    );

    loop {
        sleep(Duration::from_secs(60 * 60)).await;
    }
}
