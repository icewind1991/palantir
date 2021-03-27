use iai::black_box;
use palantir::get_metrics;
use tokio::runtime::Runtime;

fn iai_get_metrics() -> String {
    let rt = Runtime::new().unwrap();
    rt.block_on(async { black_box(get_metrics().await.unwrap()) })
}

iai::main!(iai_get_metrics);
