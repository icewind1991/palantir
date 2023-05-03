use crate::hwmon::FileSource;
use crate::sensors::Memory;
use std::fmt::Write;
use std::fs::read_to_string;
use std::str::FromStr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::thread::sleep;
use std::time::{Duration, Instant};

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

static GPU_POWER_UJ: AtomicU64 = AtomicU64::new(0);
static GPU_POWER_LAST_READ: Mutex<Option<Instant>> = Mutex::new(None);

fn get_gpu_power_elapsed() -> Option<Duration> {
    let mut last_read = GPU_POWER_LAST_READ.lock().unwrap();
    let now = Instant::now();
    let elapsed = last_read.as_ref().map(|last_read| now - *last_read);
    *last_read = Some(now);
    elapsed
}

pub fn update_gpu_power() {
    if let Ok(mut file) =
        FileSource::open("/sys/class/drm/card0/device/hwmon/hwmon0/power1_average")
    {
        loop {
            if let Some(elapsed) = get_gpu_power_elapsed() {
                let current_power: u64 = match file.read() {
                    Ok(current_power) => current_power,
                    Err(_) => {
                        return;
                    }
                };

                let elapsed_milli = elapsed.as_millis() as u64;

                let power = current_power * elapsed_milli / 1000;

                GPU_POWER_UJ.fetch_add(power, Ordering::SeqCst);
            }
            sleep(Duration::from_millis(500));
        }
    }
}

pub fn gpu_power() -> u64 {
    GPU_POWER_UJ.load(Ordering::SeqCst)
}
