use crate::data::{CpuTime, Memory, NetStats, Temperatures};
use crate::linux::hwmon::{Device, FileSource};
use crate::{Error, IoResultExt, MultiSensorSource, Result, SensorSource};
use std::fs::File;
use std::io;
use std::io::{BufRead, BufReader, ErrorKind, Read, Seek};
use sysconf::{sysconf, SysconfVariable};

pub struct TemperatureSource {
    cpu_sensors: Vec<FileSource>,
    gpu_sensors: Vec<FileSource>,
}

impl TemperatureSource {
    pub fn new() -> Result<TemperatureSource> {
        let mut cpu_sensors = Vec::new();
        let mut gpu_sensors = Vec::new();

        for device in Device::list().flatten() {
            if device.name() == "k10temp"
                || device.name() == "coretemp"
                || device.name() == "cpu_thermal"
                || device.name() == "soc_thermal"
            {
                for sensor in device.sensors().flatten() {
                    if sensor.name() == "Tdie" || sensor.name().starts_with("Core ") {
                        cpu_sensors.push(sensor.reader().context("error opening cpu temp sensor")?);
                    }
                }
            }

            if device.name() == "amdgpu" || device.name() == "gpu_thermal" {
                for sensor in device.sensors().flatten() {
                    if sensor.name() == "edge" {
                        gpu_sensors.push(sensor.reader().context("error opening gpu temp sensor")?);
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

pub fn average_sensors(sensors: &mut [FileSource]) -> f32 {
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
        let mut result = Temperatures {
            cpu: average_sensors(&mut self.cpu_sensors) / 1000.0,
            gpu: average_sensors(&mut self.gpu_sensors) / 1000.0,
        };

        if let Some(gpu) = super::gpu::nvidia::temperature() {
            result.gpu = gpu;
        }

        Ok(result)
    }
}

pub struct MemorySource {
    source: File,
    buff: String,
}

impl MemorySource {
    pub fn new() -> Result<MemorySource> {
        Ok(MemorySource {
            source: File::open("/proc/meminfo").context("error opening meminfo")?,
            buff: String::new(),
        })
    }
}

impl SensorSource for MemorySource {
    type Data = Memory;

    fn read(&mut self) -> Result<Self::Data> {
        self.buff.clear();
        self.source.rewind().context("error rewdinging meminfo")?;
        self.source
            .read_to_string(&mut self.buff)
            .context("error reading meminfo")?;

        let mut mem = Memory::default();
        for line in self.buff.lines() {
            if let Some(line) = line.strip_suffix(" kB") {
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
}

pub struct CpuTimeSource {
    source: BufReader<File>,
    buff: Vec<u8>,
    cpu_count: f32,
}

impl CpuTimeSource {
    pub fn new() -> Result<CpuTimeSource> {
        Ok(CpuTimeSource {
            source: BufReader::new(File::open("/proc/stat").context("error opening proc stats")?),
            buff: Vec::new(),
            cpu_count: sysconf(SysconfVariable::ScNprocessorsOnln)? as f32,
        })
    }
}

impl SensorSource for CpuTimeSource {
    type Data = CpuTime;

    fn read(&mut self) -> Result<Self::Data> {
        self.buff.clear();
        self.source.rewind().context("error rewinding proc")?;

        self.source
            .read_until(b'\n', &mut self.buff)
            .context("error reading proc")?;

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
            Err(Error::io(
                "invalid proc data",
                io::Error::from(ErrorKind::InvalidData),
            ))
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
            source: File::open("/proc/net/dev").context("error opening netdev")?,
            buff: String::new(),
        })
    }

    fn parse_line(line: &str) -> Result<NetStats> {
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
            Ok(NetStats {
                interface: interface.trim_end_matches(':').into(),
                bytes_sent: bytes_sent.parse()?,
                bytes_received: bytes_received.parse()?,
            })
        } else {
            Err(Error::io(
                "error reading netdev",
                ErrorKind::InvalidData.into(),
            ))
        }
    }
}

impl MultiSensorSource for NetworkSource {
    type Data = NetStats;
    type Iter<'a> = NetworkStatParser<'a>;

    fn read(&mut self) -> Result<Self::Iter<'_>> {
        self.buff.clear();
        self.source.rewind().context("error rewinding netdev")?;
        self.source
            .read_to_string(&mut self.buff)
            .context("error reading netdev")?;

        Ok(NetworkStatParser {
            lines: self.buff.lines(),
        })
    }
}

pub struct NetworkStatParser<'a> {
    lines: std::str::Lines<'a>,
}

impl<'a> Iterator for NetworkStatParser<'a> {
    type Item = Result<NetStats>;

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
