use crate::SensorData;
use std::array::IntoIter;
use std::borrow::Cow;
use std::fmt::Write;

#[derive(Debug, Clone, Default)]
pub struct Temperatures {
    pub cpu: f32,
    pub gpu: f32,
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

#[derive(Debug, Clone, Default)]
pub struct GpuMemory {
    pub total: u64,
    pub free: u64,
}

impl SensorData for GpuMemory {
    fn write<W: Write>(&self, mut w: W, hostname: &str) {
        writeln!(
            &mut w,
            "gpu_memory_total{{host=\"{}\"}} {}",
            hostname, self.total
        )
        .ok();
        writeln!(
            &mut w,
            "gpu_memory_free{{host=\"{}\"}} {}",
            hostname, self.free
        )
        .ok();
    }
}

pub struct CpuTime(pub f32);

impl SensorData for CpuTime {
    fn write<W: Write>(&self, mut w: W, hostname: &str) {
        writeln!(w, "cpu_time{{host=\"{}\"}} {:.3}", hostname, self.0).ok();
    }
}

#[derive(Debug, Clone, Default)]
pub struct NetStats {
    pub interface: String,
    pub bytes_sent: u64,
    pub bytes_received: u64,
}

impl SensorData for NetStats {
    fn write<W: Write>(&self, mut w: W, hostname: &str) {
        if self.bytes_received > 0 || self.bytes_sent > 0 {
            writeln!(
                &mut w,
                "net_sent{{host=\"{}\", network=\"{}\"}} {}",
                hostname, self.interface, self.bytes_sent
            )
            .ok();
            writeln!(
                &mut w,
                "net_received{{host=\"{}\", network=\"{}\"}} {}",
                hostname, self.interface, self.bytes_received
            )
            .ok();
        }
    }
}

pub struct GpuUsage {
    pub system: Cow<'static, str>,
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

#[derive(Debug, Clone, Default)]
pub struct DiskStats {
    pub interface: String,
    pub bytes_sent: u64,
    pub bytes_received: u64,
}

impl SensorData for DiskStats {
    fn write<W: Write>(&self, mut w: W, hostname: &str) {
        if self.bytes_received > 0 || self.bytes_sent > 0 {
            writeln!(
                &mut w,
                "disk_sent{{host=\"{}\", disk=\"{}\"}} {}",
                hostname, self.interface, self.bytes_sent
            )
            .ok();
            writeln!(
                &mut w,
                "disk_received{{host=\"{}\", disk=\"{}\"}} {}",
                hostname, self.interface, self.bytes_received
            )
            .ok();
        }
    }
}

#[derive(Clone, Debug)]
pub struct DiskUsage {
    pub name: String,
    pub size: u64,
    pub free: u64,
}

impl SensorData for DiskUsage {
    fn write<W: Write>(&self, mut w: W, hostname: &str) {
        if self.size > 0 {
            writeln!(
                &mut w,
                "disk_size{{host=\"{}\", disk=\"{}\"}} {}",
                hostname, self.name, self.size
            )
            .ok();
            writeln!(
                &mut w,
                "disk_free{{host=\"{}\", disk=\"{}\"}} {}",
                hostname, self.name, self.free
            )
            .ok();
        }
    }
}
