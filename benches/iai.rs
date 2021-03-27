use iai::black_box;
use palantir::get_metrics;
use palantir::heim::temperatures;
use palantir::zfs::pools;
use tokio::runtime::Runtime;

fn iai_get_metrics() -> String {
    let rt = Runtime::new().unwrap();
    rt.block_on(async { black_box(get_metrics().await.unwrap()) })
}

fn iai_zfs_pool() {
    black_box(pools().collect::<Vec<_>>());
}

fn iai_temperatures() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async { black_box(temperatures().await).unwrap() });
}

iai::main!(iai_get_metrics, iai_zfs_pool, iai_temperatures);
