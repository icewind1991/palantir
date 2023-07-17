use crate::data::{GpuMemory, GpuUsage};
use crate::linux::hwmon::FileSource;
use std::borrow::Cow;
use std::fs::{read_dir, read_to_string};
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::thread::sleep;
use std::time::{Duration, Instant};
use tracing::{info, warn};

pub mod nvidia;

fn read_num<T: FromStr>(path: &str) -> Option<T> {
    read_to_string(path).ok()?.trim().parse().ok()
}

pub fn memory() -> Option<GpuMemory> {
    if let Some(nv_mem) = nvidia::memory() {
        return Some(nv_mem);
    }
    // 1 gpu should be enough for everyone
    let used = read_num::<u64>("/sys/class/drm/card0/device/mem_info_vram_used")?;
    let total = read_num("/sys/class/drm/card0/device/mem_info_vram_total")?;
    Some(GpuMemory {
        total,
        free: total - used,
    })
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
            system: Cow::Borrowed(system),
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

fn find_gpu_sensor() -> Option<PathBuf> {
    read_dir("/sys/class/drm/card0/device/hwmon")
        .ok()?
        .flatten()
        .find_map(|hwmon| {
            let path = hwmon.path().join("power1_average");
            path.exists().then_some(path)
        })
}

pub fn update_gpu_power() {
    if let Some(Ok(mut file)) = find_gpu_sensor().map(FileSource::open) {
        loop {
            if let Some(elapsed) = get_gpu_power_elapsed() {
                let current_power: u64 = match file.read() {
                    Ok(current_power) => current_power,
                    Err(_) => {
                        warn!("failed to read gpu power sensor");
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
    info!("no gpu sensor");
}

pub fn gpu_power() -> u64 {
    GPU_POWER_UJ.load(Ordering::SeqCst)
}
