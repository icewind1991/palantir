use std::fs::{read_dir, read_to_string, File};
use std::io;
use std::io::{ErrorKind, Read, Seek};
use std::path::{Path, PathBuf};
use std::str::FromStr;
use tracing::{debug, instrument, warn};

fn read_to_string_trimmed(path: &Path) -> io::Result<String> {
    let mut s = read_to_string(path)?;
    let len = s.trim().len();
    s.truncate(len);
    Ok(s)
}

pub struct FileSource {
    path: PathBuf,
    buff: String,
    file: File,
}

impl FileSource {
    #[instrument(skip_all, fields(path = ?path.as_ref()))]
    pub fn open<P: AsRef<Path>>(path: P) -> io::Result<FileSource> {
        let path = path.as_ref();
        debug!("opening sensor");
        Ok(FileSource {
            path: path.into(),
            buff: String::with_capacity(32),
            file: File::open(path).map_err(|e| {
                warn!("failed to open sensor {}", path.display());
                e
            })?,
        })
    }

    pub fn read<T>(&mut self) -> io::Result<T>
    where
        T: FromStr,
        <T as FromStr>::Err: std::error::Error + Send + Sync + 'static,
    {
        match self.try_read() {
            Ok(result) => Ok(result),
            Err(e) => {
                warn!(
                    "failed to read sensor {}: {e:#}, reopening",
                    self.path.display()
                );
                self.reopen()?;
                self.try_read()
            }
        }
    }

    fn try_read<T>(&mut self) -> io::Result<T>
    where
        T: FromStr,
        <T as FromStr>::Err: std::error::Error + Send + Sync + 'static,
    {
        self.buff.clear();
        self.file.rewind()?;
        self.file.read_to_string(&mut self.buff)?;
        self.buff
            .trim()
            .parse()
            .map_err(|e| io::Error::new(ErrorKind::InvalidData, e))
    }

    pub fn reopen(&mut self) -> io::Result<()> {
        self.file = File::open(&self.path).map_err(|e| {
            warn!("failed to open sensor {}", self.path.display());
            e
        })?;
        Ok(())
    }
}

pub struct Device {
    base_path: PathBuf,
    name: String,
}

impl Device {
    pub fn new(path: PathBuf) -> io::Result<Device> {
        let name = read_to_string_trimmed(&path.join("name"))?;
        Ok(Device {
            base_path: path,
            name,
        })
    }

    pub fn list() -> impl Iterator<Item = io::Result<Device>> {
        let sensors = read_dir("/sys/class/hwmon").into_iter().flatten();
        sensors.map(|device| device.and_then(|device| Device::new(device.path())))
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn sensors(&self) -> impl Iterator<Item = io::Result<Sensor>> {
        // determine early to avoid borrowing &self in iterator
        let is_cpu_thermal = self.name == "cpu_thermal" || self.name == "soc_thermal";
        let is_gpu_thermal = self.name == "gpu_thermal";

        let sensors = read_dir(&self.base_path).into_iter().flatten();
        sensors
            .filter_map(|sensor| {
                let sensor = match sensor {
                    Ok(sensor) => sensor,
                    Err(e) => return Some(Err(e)),
                };

                if sensor.file_name().to_str()?.ends_with("_input") {
                    Some(Ok(sensor.path()))
                } else {
                    None
                }
            })
            .map(move |path| {
                let path = path?;

                let input_name = path.file_name().unwrap().to_str().unwrap();

                // rpi/rk3588 cpu_thermal doesn't have labels, so we hardcode one
                if is_cpu_thermal && input_name == "temp1_input" {
                    return Ok(Sensor {
                        input_path: path,
                        name: "Tdie".into(),
                    });
                }
                if is_gpu_thermal && input_name == "temp1_input" {
                    return Ok(Sensor {
                        input_path: path,
                        name: "edge".into(),
                    });
                }

                let base_name = input_name.trim_end_matches("_input");

                let label_name = path.with_file_name(format!("{base_name}_label"));
                let name = read_to_string_trimmed(&label_name)?;

                Ok(Sensor {
                    input_path: path,
                    name,
                })
            })
    }
}

pub struct Sensor {
    input_path: PathBuf,
    name: String,
}

impl Sensor {
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn reader(&self) -> io::Result<FileSource> {
        FileSource::open(&self.input_path)
    }
}
