use crate::data::ProcData;
use crate::linux::sensors::MemorySource;
use crate::{MultiSensorSource, Result, SensorSource};
use procfs::page_size;
use procfs::process::all_processes;
use std::vec::IntoIter;

#[derive(Default)]
pub struct ProcSource {
    page_size: u64,
    page_cutoff: u64,
}

impl ProcSource {
    pub fn new() -> Result<Self> {
        let total_memory = MemorySource::new()?.read()?.total;
        let page_size = page_size();

        Ok(ProcSource {
            page_size,
            // output processes that use >1% of memory
            page_cutoff: (total_memory / 100) / page_size,
        })
    }
}

impl MultiSensorSource for ProcSource {
    type Data = ProcData;
    type Iter<'a> = IntoIter<Result<ProcData>>;

    fn read(&mut self) -> Result<Self::Iter<'_>> {
        Ok(all_processes()?
            .flatten()
            .flat_map(|proc| proc.stat())
            .filter(|stat| stat.rss > self.page_cutoff)
            .map(|stat| {
                Ok(ProcData {
                    pid: stat.pid,
                    name: stat.comm,
                    rss_memory: stat.rss * self.page_size,
                })
            })
            .collect::<Vec<_>>()
            .into_iter())
    }
}
