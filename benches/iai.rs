use iai::black_box;
use palantir::get_metrics;
use palantir::sensors::{cpu_time, disk_stats, disk_usage, memory, network_stats, temperatures};
use palantir::zfs::pools;

fn iai_get_metrics() -> String {
    black_box(get_metrics().unwrap())
}

fn iai_zfs_pool() {
    black_box(pools().collect::<Vec<_>>());
}

fn iai_temperatures() {
    black_box(temperatures()).unwrap();
}
fn iai_network_stats() {
    black_box(network_stats().unwrap().map(black_box).count());
}
fn iai_disk_stats() {
    black_box(disk_stats().unwrap().map(black_box).count());
}
fn iai_disk_usage() {
    black_box(disk_usage().unwrap().map(black_box).count());
}
fn iai_memory() {
    black_box(memory()).unwrap();
}
fn iai_cpu_time() {
    black_box(cpu_time()).unwrap();
}

iai::main!(
    iai_get_metrics,
    iai_zfs_pool,
    iai_temperatures,
    iai_network_stats,
    iai_disk_stats,
    iai_disk_usage,
    iai_memory,
    iai_cpu_time
);
