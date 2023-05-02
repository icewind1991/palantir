use crate::gpu::GpuUsage;
use crate::sensors::Memory;
use nvml_wrapper::enum_wrappers::device::TemperatureSensor;
use nvml_wrapper::{Device, Nvml};
use once_cell::sync::Lazy;

static NVIDIA: Lazy<Option<Nvml>> = Lazy::new(|| Nvml::init().ok());

fn device() -> Option<Device<'static>> {
    NVIDIA.as_ref()?.device_by_index(0).ok()
}

pub fn temperature() -> Option<f32> {
    let temp = device()?.temperature(TemperatureSensor::Gpu).ok()?;
    Some(temp as f32)
}

pub fn power() -> Option<u64> {
    device()?.total_energy_consumption().ok()
}

pub fn memory() -> Option<Memory> {
    let mem = device()?.memory_info().ok()?;
    Some(Memory {
        total: mem.total,
        free: mem.free,
        available: mem.free,
    })
}

pub fn utilization() -> impl Iterator<Item = GpuUsage> {
    let sources = if let Some(device) = device() {
        let utilization = device.utilization_rates().ok();
        [
            ("compute", utilization.as_ref().map(|u| u.gpu)),
            ("memory", utilization.as_ref().map(|u| u.gpu)),
            (
                "encode",
                device.encoder_utilization().ok().map(|u| u.utilization),
            ),
            (
                "decode",
                device.decoder_utilization().ok().map(|u| u.utilization),
            ),
        ]
    } else {
        [("", None); 4]
    };
    sources.into_iter().flat_map(|(system, usage)| {
        Some(GpuUsage {
            system,
            usage: usage?,
        })
    })
}
