use color_eyre::{Report, Result};
use palantir::get_metrics;
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

async fn serve_metrics() -> Result<String, Rejection> {
    get_metrics()
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

    let metrics = warp::path!("metrics").and_then(serve_metrics);

    warp::serve(metrics).run(([0, 0, 0, 0], host_port)).await;
    Ok(())
}
