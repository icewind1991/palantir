use std::ffi::NulError;
use std::fmt::Write;
use std::num::{ParseFloatError, ParseIntError};
use std::str::Utf8Error;
use std::string::FromUtf8Error;

pub mod data;
pub mod docker;

#[cfg(not(feature = "sysinfo"))]
mod linux;
#[cfg(feature = "sysinfo")]
mod sys;

#[cfg(not(feature = "sysinfo"))]
pub use linux::{get_metrics, Sensors};
#[cfg(feature = "sysinfo")]
pub use sys::{get_metrics, Sensors};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Other(String),
    #[error("Non UTF8 hostname")]
    InvalidHostName,
    #[error(transparent)]
    InvalidIntData(#[from] ParseIntError),
    #[error(transparent)]
    InvalidFloatData(#[from] ParseFloatError),
    #[error(transparent)]
    InvalidStringData(#[from] Utf8Error),
    #[error(transparent)]
    InvalidCStringData(#[from] NulError),
    #[error("Failed to query vfs stats")]
    StatVfs,
}

impl From<FromUtf8Error> for Error {
    fn from(err: FromUtf8Error) -> Self {
        Self::InvalidStringData(err.utf8_error())
    }
}

pub type Result<T, E = Error> = std::result::Result<T, E>;

pub trait SensorData {
    /// Write sensor data in prometheus compatible format
    fn write<W: Write>(&self, w: W, hostname: &str);
}

pub trait SensorSource {
    type Data: SensorData;

    fn read(&mut self) -> Result<Self::Data>;
}

pub trait MultiSensorSource {
    type Data: SensorData;
    type Iter<'a>: Iterator<Item = Result<Self::Data>>
    where
        Self: 'a;

    fn read(&mut self) -> Result<Self::Iter<'_>>;
}

pub fn hostname() -> Result<String> {
    hostname::get()?
        .into_string()
        .map_err(|_| Error::InvalidHostName)
}
