#[cfg(not(target_os = "windows"))]
use procfs::ProcError;
use std::ffi::NulError;
use std::fmt::Write;
use std::num::{ParseFloatError, ParseIntError};
use std::str::Utf8Error;
use std::string::FromUtf8Error;

pub mod data;
pub mod docker;

#[cfg(not(target_os = "windows"))]
pub mod linux;
#[cfg(target_os = "windows")]
pub mod win;

#[cfg(not(target_os = "windows"))]
pub use linux::{get_metrics, Sensors};
#[cfg(target_os = "windows")]
pub use win::{get_metrics, Sensors};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{1}: {0}")]
    Io(std::io::Error, &'static str),
    #[error("{1}: {0}")]
    Os(std::io::Error, &'static str),
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
    #[cfg(not(target_os = "windows"))]
    #[error(transparent)]
    Proc(#[from] ProcError),
    #[error("Failed to query vfs stats")]
    StatVfs,
    #[cfg(target_os = "windows")]
    #[error(transparent)]
    Wmi(#[from] wmi::WMIError),
    #[cfg(target_os = "windows")]
    #[error("{0}")]
    Reg(String),
}

impl Error {
    pub fn last_os_error(context: &'static str) -> Error {
        let err = std::io::Error::last_os_error();
        Error::Os(err, context)
    }

    pub fn io(context: &'static str, err: std::io::Error) -> Error {
        Error::Io(err, context)
    }
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
    hostname::get()
        .context("error getting hostname")?
        .into_string()
        .map_err(|_| Error::InvalidHostName)
}

pub trait IoResultExt<T> {
    fn context(self, context: &'static str) -> Result<T, Error>;
}

impl<T> IoResultExt<T> for Result<T, std::io::Error> {
    fn context(self, context: &'static str) -> Result<T, Error> {
        self.map_err(|e| Error::io(context, e))
    }
}
