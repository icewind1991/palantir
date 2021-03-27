use crate::heim::DiskUsage;
use color_eyre::Result;
use std::process::Command;
use tokio::task::spawn_blocking;

pub async fn pools() -> Result<Vec<DiskUsage>> {
    spawn_blocking(move || {
        let mut z = Command::new("zpool");
        z.args(&["list", "-p", "-H", "-o", "name,size,free"]);
        let out = match z.output() {
            Ok(out) => out,
            Err(_) => return Ok(Vec::new()),
        };
        if out.status.success() {
            let output = String::from_utf8(out.stdout)?;
            Ok(output
                .lines()
                .flat_map(|line| {
                    let mut parts = line.split_ascii_whitespace();
                    let name = parts.next()?.to_string();
                    let size = parts.next()?.parse().ok()?;
                    let free = parts.next()?.parse().ok()?;
                    Some(DiskUsage { name, size, free })
                })
                .collect())
        } else {
            Ok(Vec::new())
        }
    })
    .await?
}
