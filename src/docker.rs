use bollard::container::{Stats, StatsOptions};
use bollard::models::ContainerSummary;
use bollard::Docker;
use color_eyre::Result;
use futures_util::future::ready;
use futures_util::stream::{FuturesUnordered, Stream, StreamExt};
use std::collections::HashMap;
use std::fmt::Write;

#[derive(Debug)]
pub struct Container {
    name: String,
    image: String,
    memory: u64,
    cpu_time: f64,
    network_sent: u64,
    network_received: u64,
}

impl Container {
    pub fn write<W: Write>(&self, mut w: W, hostname: &str) {
        writeln!(
            &mut w,
            "container_memory{{host=\"{}\", container=\"{}\", image=\"{}\"}} {}",
            hostname, self.name, self.image, self.memory
        )
        .ok();
        writeln!(
            &mut w,
            "container_cpu_time{{host=\"{}\", container=\"{}\", image=\"{}\"}} {:.3}",
            hostname, self.name, self.image, self.cpu_time
        )
        .ok();
        writeln!(
            &mut w,
            "container_net_sent{{host=\"{}\", container=\"{}\", image=\"{}\"}} {:.3}",
            hostname, self.name, self.image, self.network_sent
        )
        .ok();
        writeln!(
            &mut w,
            "container_net_received{{host=\"{}\", container=\"{}\", image=\"{}\"}} {:.3}",
            hostname, self.name, self.image, self.network_received
        )
        .ok();
    }

    fn from(stats: Stats, container: ContainerSummary) -> Self {
        Container {
            name: stats.name,
            image: container.image.unwrap_or_default(),
            memory: stats.memory_stats.usage.unwrap_or_default(),
            cpu_time: stats.cpu_stats.cpu_usage.total_usage as f64
                / 1_000_000_000.0
                / stats.cpu_stats.online_cpus.unwrap_or(1) as f64,
            network_sent: stats
                .networks
                .as_ref()
                .into_iter()
                .flat_map(HashMap::values)
                .map(|stats| stats.tx_bytes)
                .sum(),
            network_received: stats
                .networks
                .as_ref()
                .into_iter()
                .flat_map(HashMap::values)
                .map(|stats| stats.rx_bytes)
                .sum(),
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
    let containers = docker.list_containers::<String>(None).await?;
    Ok(containers
        .into_iter()
        .map(move |container| {
            let docker = docker.clone();
            async move {
                let id = container.id.as_ref().unwrap();
                let stats: Stats = docker
                    .stats(
                        id,
                        Some(StatsOptions {
                            stream: false,
                            one_shot: true,
                        }),
                    )
                    .next()
                    .await?
                    .ok()?;
                Some(Container::from(stats, container))
            }
        })
        .collect::<FuturesUnordered<_>>()
        .filter_map(|opt| ready(opt)))
}
