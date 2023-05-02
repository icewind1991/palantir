use crate::sensors::Memory;
use std::fmt::Write;
use std::fs::read_to_string;
use std::str::FromStr;

pub mod nvidia;

pub fn gpu_metrics<W: Write>(mut out: W, hostname: &str) {
    if let Some(memory) = memory() {
        writeln!(
            &mut out,
            "gpu_memory_total{{host=\"{}\"}} {}",
            hostname, memory.total
        )
        .ok();
        writeln!(
            &mut out,
            "gpu_memory_free{{host=\"{}\"}} {}",
            hostname, memory.free
        )
        .ok();
    }

    for usage in utilization() {
        usage.write(&mut out, hostname);
    }
}

fn read_num<T: FromStr>(path: &str) -> Option<T> {
    read_to_string(path).ok()?.trim().parse().ok()
}

pub fn memory() -> Option<Memory> {
    if let Some(nv_mem) = nvidia::memory() {
        return Some(nv_mem);
    }
    // 1 gpu should be enough for everyone
    let used = read_num::<u64>("/sys/class/drm/card0/device/mem_info_vram_used")?;
    let total = read_num("/sys/class/drm/card0/device/mem_info_vram_total")?;
    Some(Memory {
        total,
        free: total - used,
        available: total - used,
    })
}

pub struct GpuUsage {
    pub system: &'static str,
    pub usage: u32,
}

impl GpuUsage {
    pub fn write<W: Write>(&self, mut w: W, hostname: &str) {
        writeln!(
            &mut w,
            r#"gpu_usage{{host="{}", system="{}"}} {:.3}"#,
            hostname, self.system, self.usage,
        )
        .ok();
    }
}

pub fn utilization() -> impl Iterator<Item = GpuUsage> {
    let nv_usage = nvidia::utilization();

    let sources = [
        (
            "memory",
            read_num("/sys/class/drm/card0/device/mem_busy_percent"),
        ),
        (
            "compute",
            read_num("/sys/class/drm/card0/device/gpu_busy_percent"),
        ),
    ];
    let drm = sources.into_iter().flat_map(|(system, usage)| {
        Some(GpuUsage {
            system,
            usage: usage?,
        })
    });
    drm.chain(nv_usage)
}
