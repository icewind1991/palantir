use color_eyre::Result;
use libzetta::zpool::{ZpoolEngine, ZpoolOpen3};
use tokio::task::spawn_blocking;

#[derive(Clone, Debug)]
pub struct ZfsPool {
    pub name: String,
    pub size: usize,
    pub free: usize,
}

pub struct ZFS {
    engine: ZpoolOpen3,
}

impl Default for ZFS {
    fn default() -> Self {
        ZFS {
            engine: ZpoolOpen3::default(),
        }
    }
}

impl ZFS {
    pub async fn pools(self) -> Result<Vec<ZfsPool>> {
        spawn_blocking(move || {
            let pools = self.engine.all()?;
            pools
                .into_iter()
                .map(|pool| {
                    let props = self.engine.read_properties(pool.name())?;
                    Ok(ZfsPool {
                        name: pool.name().to_string(),
                        size: *props.size(),
                        free: *props.size() * (*props.capacity() as usize) / 100,
                    })
                })
                .collect()
        })
        .await?
    }
}
