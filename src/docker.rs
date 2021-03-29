use bollard::container::Stats;
use bollard::Docker;
use color_eyre::Result;
use futures_util::stream::{iter, Stream, StreamExt};
use std::fmt::Write;

pub struct Container {
    name: String,
    memory: u64,
    cpu_time: u64,
}

impl From<Stats> for Container {
    fn from(stats: Stats) -> Self {
        Container {
            name: stats.name,
            memory: stats.memory_stats.usage.unwrap_or_default(),
            cpu_time: stats.cpu_stats.cpu_usage.total_usage,
        }
    }
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
            "container_cpu_time{{host=\"{}\", container=\"{}\"}} {}",
            hostname, self.name, self.cpu_time
        )
        .ok();
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
    Ok(iter(containers.into_iter()).filter_map(move |container| {
        let docker = docker.clone();
        async move {
            let id = container.id.unwrap();
            let stats: Stats = docker.stats(&id, None).next().await?.ok()?;
            Some(stats.into())
        }
    }))
}
