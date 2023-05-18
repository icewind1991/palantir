use crate::{Error, Result};
use serde::Deserialize;
use winreg::enums::*;
use winreg::RegKey;

#[derive(Debug, Deserialize)]
struct GpuInfo {
    #[serde(rename = "HardwareInformation.qwMemorySize")]
    memory_size: Option<u64>,
}

pub fn total_gpu_memory() -> Result<u64> {
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    let mut mem = 0;
    for i in 0..3 {
        if let Ok(gpu_key) = hklm.open_subkey(
            format!("SYSTEM\\ControlSet001\\Control\\Class\\{{4d36e968-e325-11ce-bfc1-08002be10318}}\\{:04}", i),
        ) {
            let info: GpuInfo = gpu_key.decode().map_err(|e| Error::Reg(e.to_string()))?;
            mem += info.memory_size.unwrap_or_default();
        }
    }
    Ok(mem)
}
