use crate::data::CpuTime;
use crate::{Error, Result, SensorSource};
use winapi::shared::minwindef;
use winapi::um::{processthreadsapi, winbase, winnt};

pub struct CpuTimeSource {
    cpu_count: f32,
}

impl CpuTimeSource {
    pub fn new() -> Result<CpuTimeSource> {
        Ok(CpuTimeSource {
            cpu_count: dbg!(cpu_count()?),
        })
    }
}

impl SensorSource for CpuTimeSource {
    type Data = CpuTime;

    fn read(&mut self) -> Result<Self::Data> {
        let mut user = minwindef::FILETIME::default();
        let mut kernel = minwindef::FILETIME::default();
        let mut idle = minwindef::FILETIME::default();

        let result =
            unsafe { processthreadsapi::GetSystemTimes(&mut idle, &mut kernel, &mut user) };

        if result == 0 {
            Err(Error::last_os_error("GetSystemTimes"))
        } else {
            let user = time_to_float(user);
            let idle = time_to_float(idle);
            // Same as `psutil` subtracting idle time
            // and leaving only busy kernel time
            let system = time_to_float(kernel) - idle;

            Ok(CpuTime((user + system) / self.cpu_count))
        }
    }
}

fn time_to_float(time: minwindef::FILETIME) -> f32 {
    const HI_T: f64 = 429.496_729_6;
    const LO_T: f64 = 1e-7;

    let low = LO_T * f64::from(time.dwLowDateTime);
    HI_T.mul_add(f64::from(time.dwHighDateTime), low) as f32
}

fn cpu_count() -> Result<f32> {
    let result = unsafe { winbase::GetActiveProcessorCount(winnt::ALL_PROCESSOR_GROUPS) };

    if result > 0 {
        Ok(result as f32)
    } else {
        Err(Error::last_os_error("GetActiveProcessorCount"))
    }
}
