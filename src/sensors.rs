use crate::disk::IoStats;
use crate::hwmon::{Device, FileSource};
use crate::{Error, MultiSensorSource, Result, SensorData, SensorSource};
use std::array::IntoIter;
use std::fmt::Write;
use std::fs::File;
use std::io;
use std::io::{BufRead, BufReader, ErrorKind, Read, Seek};
use sysconf::{sysconf, SysconfVariable};

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
    pub fn new() -> Result<TemperatureSource> {
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

    fn read(&mut self) -> Result<Self::Data> {
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

pub struct CpuTime(f32);

impl SensorData for CpuTime {
    fn write<W: Write>(&self, mut w: W, hostname: &str) {
        writeln!(w, "cpu_time{{host=\"{}\"}} {:.3}", hostname, self.0).ok();
    }
}

pub struct CpuTimeSource {
    source: BufReader<File>,
    buff: Vec<u8>,
    cpu_count: f32,
}

impl CpuTimeSource {
    pub fn new() -> Result<CpuTimeSource> {
        Ok(CpuTimeSource {
            source: BufReader::new(File::open("/proc/stat")?),
            buff: Vec::new(),
            cpu_count: sysconf(SysconfVariable::ScNprocessorsOnln)? as f32,
        })
    }
}

impl SensorSource for CpuTimeSource {
    type Data = CpuTime;

    fn read(&mut self) -> Result<Self::Data> {
        self.buff.clear();
        self.source.rewind()?;

        self.source.read_until(b'\n', &mut self.buff)?;

        let line = std::str::from_utf8(&self.buff)?;

        let mut parts = line.split_ascii_whitespace();
        if let (_cpu, Some(user), _nice, Some(system)) =
            (parts.next(), parts.next(), parts.next(), parts.next())
        {
            let user: f32 = user.parse()?;
            let system: f32 = system.parse()?;
            let clock_ticks = sysconf(SysconfVariable::ScClkTck)?;
            Ok(CpuTime(
                (user + system) / (clock_ticks as f32) / self.cpu_count,
            ))
        } else {
            Err(io::Error::from(ErrorKind::InvalidData).into())
        }
    }
}

pub struct NetworkSource {
    source: File,
    buff: String,
}

impl NetworkSource {
    pub fn new() -> Result<NetworkSource> {
        Ok(NetworkSource {
            source: File::open("/proc/net/dev")?,
            buff: String::new(),
        })
    }

    fn parse_line(line: &str) -> Result<IoStats> {
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
            Ok(IoStats {
                interface: interface.trim_end_matches(':').into(),
                bytes_sent: bytes_sent.parse()?,
                bytes_received: bytes_received.parse()?,
            })
        } else {
            Err(Error::Io(ErrorKind::InvalidData.into()))
        }
    }
}

impl MultiSensorSource for NetworkSource {
    type Data = IoStats;
    type Iter<'a> = NetworkStatParser<'a>;

    fn read(&mut self) -> Result<Self::Iter<'_>> {
        self.buff.clear();
        self.source.rewind()?;
        self.source.read_to_string(&mut self.buff)?;

        Ok(NetworkStatParser {
            lines: self.buff.lines(),
        })
    }
}

pub struct NetworkStatParser<'a> {
    lines: std::str::Lines<'a>,
}

impl<'a> Iterator for NetworkStatParser<'a> {
    type Item = Result<IoStats>;

    fn next(&mut self) -> Option<Self::Item> {
        let line = loop {
            let line = self.lines.next()?;
            let trimmed = line.trim_start();
            if trimmed.starts_with("en") || trimmed.starts_with("eth") || trimmed.starts_with("wlp")
            {
                break trimmed;
            }
        };

        Some(NetworkSource::parse_line(line))
    }
}

pub fn hostname() -> Result<String> {
    hostname::get()?
        .into_string()
        .map_err(|_| Error::InvalidHostName)
}
