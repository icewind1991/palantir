use bollard::Docker;
use color_eyre::{Report, Result};
use futures_util::pin_mut;
use futures_util::StreamExt;
use palantir::docker::{get_docker, stat, Container};
use palantir::get_metrics;
use palantir::power::power_usage;
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
    let host_port: u16 = dotenv::var("PORT")
        .ok()
        .map(|port| port.parse())
        .transpose()?
        .unwrap_or(80);

    ctrlc::set_handler(move || {
        std::process::exit(0);
    })
    .expect("Error setting Ctrl-C handler");

    let docker = get_docker().await;
    let docker = warp::any().map(move || docker.clone());

    let metrics = warp::path!("metrics").and(docker).and_then(serve_metrics);

    warp::serve(metrics).run(([0, 0, 0, 0], host_port)).await;
    Ok(())
}
