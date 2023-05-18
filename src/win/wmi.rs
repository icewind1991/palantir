use crate::data::DiskStats;
use crate::Result;
use serde::Deserialize;
use std::collections::HashMap;
use wmi::{COMLibrary, WMIConnection};

pub struct WmiSensor {
    wmi_con: WMIConnection,
}

impl WmiSensor {
    pub fn new() -> Result<Self> {
        let com_con = COMLibrary::new()?;
        let wmi_con = WMIConnection::new(com_con.into())?;

        Ok(WmiSensor { wmi_con })
    }

    pub fn gpu_mem(&self) -> Result<u64> {
        #[derive(Deserialize, Debug)]
        #[allow(non_camel_case_types)]
        struct Win32_PerfFormattedData_GPUPerformanceCounters_GPUAdapterMemory {
            #[serde(rename = "DedicatedUsage")]
            dedicated_usage: u64,
        }

        let results: Vec<Win32_PerfFormattedData_GPUPerformanceCounters_GPUAdapterMemory> =
            self.wmi_con.query()?;
        Ok(results.iter().map(|result| result.dedicated_usage).sum())
    }

    pub fn gpu_usage(&self) -> Result<HashMap<String, u32>> {
        #[derive(Deserialize, Debug)]
        #[allow(non_camel_case_types)]
        struct Win32_PerfFormattedData_GPUPerformanceCounters_GPUEngine {
            #[serde(rename = "Name")]
            name: String,
            #[serde(rename = "UtilizationPercentage")]
            usage: u32,
        }

        let results: Vec<Win32_PerfFormattedData_GPUPerformanceCounters_GPUEngine> =
            self.wmi_con.query()?;

        let mut data = HashMap::default();

        for result in results {
            if let Some(eng_type) = result.name.split("_engtype_").skip(1).next() {
                let entry = data.entry(eng_type.to_string()).or_default();
                *entry += result.usage;
            }
        }

        Ok(data)
    }

    pub fn disk_usage(&self) -> Result<Option<DiskStats>> {
        #[derive(Deserialize, Debug)]
        #[allow(non_camel_case_types)]
        struct Win32_PerfRawData_Counters_FileSystemDiskActivity {
            #[serde(rename = "Name")]
            name: String,
            #[serde(rename = "FileSystemBytesRead")]
            read: u64,
            #[serde(rename = "FileSystemBytesWritten")]
            written: u64,
        }

        let results: Vec<Win32_PerfRawData_Counters_FileSystemDiskActivity> =
            self.wmi_con.query()?;
        for result in results {
            if result.name == "_Total" {
                return Ok(Some(DiskStats {
                    interface: "Total".to_string(),
                    bytes_sent: result.written,
                    bytes_received: result.read,
                }));
            }
        }
        Ok(None)
    }
}
