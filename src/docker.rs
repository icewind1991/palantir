use crate::sensors::clock_ticks;
use bollard::container::{Stats, StatsOptions};
use bollard::Docker;
use color_eyre::Result;
use futures_util::future::ready;
use futures_util::stream::{FuturesUnordered, Stream, StreamExt};
use std::fmt::Write;

#[derive(Debug)]
pub struct Container {
    name: String,
    memory: u64,
    cpu_time: f64,
}

impl Container {
    pub fn write<W: Write>(&self, mut w: W, hostname: &str) {
        writeln!(
            &mut w,
            "container_memory{{host=\"{}\", container=\"{}\"}} {}",
            hostname, self.name, self.memory
        )
        .ok();
        writeln!(
            &mut w,
            "container_cpu_time{{host=\"{}\", container=\"{}\"}} {:.3}",
            hostname, self.name, self.cpu_time
        )
        .ok();
    }

    fn from(stats: Stats, ticks: u64) -> Self {
        Container {
            name: stats.name,
            memory: stats.memory_stats.usage.unwrap_or_default(),
            cpu_time: stats.cpu_stats.cpu_usage.total_usage as f64
                / 1_000_000.0
                / ticks as f64
                / stats.cpu_stats.online_cpus.unwrap_or(1) as f64,
        }
    }
}

pub async fn get_docker() -> Option<Docker> {
    match Docker::connect_with_local_defaults() {
        Ok(docker) => docker
            .list_containers::<String>(None)
            .await
            .ok()
            .map(|_| docker),
        Err(_) => None,
    }
}

pub async fn stat(docker: Docker) -> Result<impl Stream<Item = Container>> {
    let ticks = clock_ticks()?;
    let containers = docker.list_containers::<String>(None).await?;
    Ok(containers
        .into_iter()
        .map(move |container| {
            let docker = docker.clone();
            async move {
                let id = container.id.unwrap();
                let stats: Stats = docker
                    .stats(
                        &id,
                        Some(StatsOptions {
                            stream: false,
                            one_shot: true,
                        }),
                    )
                    .next()
                    .await?
                    .ok()?;
                Some(Container::from(stats, ticks))
            }
        })
        .collect::<FuturesUnordered<_>>()
        .filter_map(|opt| ready(opt)))
}
