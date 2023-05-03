use crate::disk::IoStats;
use crate::hwmon::{Device, FileSource};
use crate::{SensorData, SensorSource};
use color_eyre::{Report, Result};
use std::array::IntoIter;
use std::fmt::Write;
use std::fs::File;
use std::io;
use std::io::{BufRead, BufReader};

#[derive(Debug, Clone, Default)]
pub struct Temperatures {
    cpu: f32,
    gpu: f32,
}

impl IntoIterator for Temperatures {
    type Item = (&'static str, f32);
    type IntoIter = IntoIter<Self::Item, 2>;

    fn into_iter(self) -> Self::IntoIter {
        [("cpu", self.cpu), ("gpu", self.gpu)].into_iter()
    }
}

impl SensorData for Temperatures {
    fn write<W: Write>(&self, mut w: W, hostname: &str) {
        for (label, temp) in self.clone() {
            if temp != 0.0 {
                writeln!(
                    &mut w,
                    "temperature{{host=\"{}\", sensor=\"{}\"}} {:.1}",
                    hostname, label, temp
                )
                .ok();
            }
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct Memory {
    pub total: u64,
    pub free: u64,
    pub available: u64,
}

impl SensorData for Memory {
    fn write<W: Write>(&self, mut w: W, hostname: &str) {
        writeln!(
            &mut w,
            "memory_total{{host=\"{}\"}} {}",
            hostname, self.total
        )
        .ok();
        writeln!(
            &mut w,
            "memory_available{{host=\"{}\"}} {}",
            hostname, self.available
        )
        .ok();
        writeln!(&mut w, "memory_free{{host=\"{}\"}} {}", hostname, self.free).ok();
    }
}

pub struct TemperatureSource {
    cpu_sensors: Vec<FileSource>,
    gpu_sensors: Vec<FileSource>,
}

impl TemperatureSource {
    pub fn new() -> io::Result<TemperatureSource> {
        let mut cpu_sensors = Vec::new();
        let mut gpu_sensors = Vec::new();

        for device in Device::list().flatten() {
            if device.name() == "k10temp" || device.name() == "coretemp" {
                for sensor in device.sensors().flatten() {
                    if sensor.name() == "Tdie" || sensor.name().starts_with("Core ") {
                        cpu_sensors.push(sensor.reader()?);
                    }
                }
            }

            if device.name() == "amdgpu" {
                for sensor in device.sensors().flatten() {
                    if sensor.name() == "edge" {
                        gpu_sensors.push(sensor.reader()?);
                    }
                }
            }
        }

        Ok(TemperatureSource {
            cpu_sensors,
            gpu_sensors,
        })
    }
}

fn average_sensors(sensors: &mut [FileSource]) -> f32 {
    if sensors.is_empty() {
        return 0.0;
    }

    let mut total = 0.0;
    let mut count = 0.0;
    for sensor in sensors.iter_mut() {
        if let Ok(value) = sensor.read::<f32>() {
            total += value;
            count += 1.0
        }
    }
    total / count
}

impl SensorSource for TemperatureSource {
    type Data = Temperatures;

    fn read(&mut self) -> io::Result<Self::Data> {
        Ok(Temperatures {
            cpu: average_sensors(&mut self.cpu_sensors) / 1000.0,
            gpu: average_sensors(&mut self.gpu_sensors) / 1000.0,
        })
    }
}

pub fn memory() -> Result<Memory> {
    let mut meminfo = BufReader::new(File::open("/proc/meminfo")?);
    let mut mem = Memory::default();
    let mut line = String::new();
    loop {
        line.clear();
        meminfo.read_line(&mut line)?;
        if line.is_empty() {
            break;
        }
        if let Some(line) = line.strip_suffix(" kB\n") {
            if let Some(line_total) = line.strip_prefix("MemTotal: ") {
                mem.total = line_total.trim().parse::<u64>()? * 1000;
            }
            if let Some(line_free) = line.strip_prefix("MemFree: ") {
                mem.free = line_free.trim().parse::<u64>()? * 1000;
            }
            if let Some(line_available) = line.strip_prefix("MemAvailable: ") {
                mem.available = line_available.trim().parse::<u64>()? * 1000;
            }
        }
    }
    Ok(mem)
}

pub fn cpu_time() -> Result<f32> {
    let stat = BufReader::new(File::open("/proc/stat")?);
    let line = stat
        .lines()
        .next()
        .ok_or_else(|| Report::msg("Invalid /proc/stat"))??;
    let mut parts = line.split_ascii_whitespace();
    if let (_cpu, Some(user), _nice, Some(system)) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    {
        let user: f32 = user.parse()?;
        let system: f32 = system.parse()?;
        Ok((user + system) / (clock_ticks()? as f32) / (cpu_count()? as f32))
    } else {
        Err(Report::msg("Invalid /proc/stat"))
    }
}

pub fn network_stats() -> Result<impl Iterator<Item = IoStats>> {
    let stat = BufReader::new(File::open("/proc/net/dev")?);
    Ok(stat
        .lines()
        .filter_map(Result::ok)
        .filter(|line: &String| {
            let trimmed = line.trim_start();
            trimmed.starts_with("en") || trimmed.starts_with("eth")
        })
        .filter_map(|line: String| {
            let mut parts = line.trim_start().split_ascii_whitespace();
            if let (
                Some(interface),
                Some(bytes_received),
                _packets,
                _err,
                _drop,
                _fifo,
                _frame,
                _compressed,
                _multicast,
                Some(bytes_sent),
            ) = (
                parts.next(),
                parts.next(),
                parts.next(),
                parts.next(),
                parts.next(),
                parts.next(),
                parts.next(),
                parts.next(),
                parts.next(),
                parts.next(),
            ) {
                Some(IoStats {
                    interface: interface.trim_end_matches(':').into(),
                    bytes_sent: bytes_sent.parse().ok()?,
                    bytes_received: bytes_received.parse().ok()?,
                })
            } else {
                None
            }
        }))
}

pub fn hostname() -> Result<String> {
    hostname::get()?
        .into_string()
        .map_err(|_| Report::msg("non utf8 hostname"))
}

pub fn clock_ticks() -> Result<u64> {
    let result = unsafe { libc::sysconf(libc::_SC_CLK_TCK) };

    if result > 0 {
        Ok(result as u64)
    } else {
        Err(Report::msg("Failed to get clock ticks"))
    }
}

fn cpu_count() -> Result<u64> {
    let result = unsafe { libc::sysconf(libc::_SC_NPROCESSORS_ONLN) };

    if result < 0 {
        Err(Report::msg("Failed to get cpu count"))
    } else {
        Ok(result as u64)
    }
}
