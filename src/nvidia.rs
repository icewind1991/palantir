use nvml_wrapper::enum_wrappers::device::TemperatureSensor;
use nvml_wrapper::Nvml;
use once_cell::sync::Lazy;

static NVIDIA: Lazy<Option<Nvml>> = Lazy::new(|| Nvml::init().ok());

pub fn temperature() -> Option<f32> {
    let device = NVIDIA.as_ref()?.device_by_index(0).ok()?;
    let temp = device.temperature(TemperatureSensor::Gpu).ok()?;
    Some(temp as f32)
}

pub fn power() -> Option<u64> {
    let device = NVIDIA.as_ref()?.device_by_index(0).ok()?;
    device.total_energy_consumption().ok()
}
