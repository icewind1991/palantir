use crate::data::Temperatures;
use crate::hostname;
use crate::Result;
use std::fmt::Write;
use sysinfo::{ComponentExt, System, SystemExt};

pub struct Sensors {
    pub hostname: String,
}

impl Sensors {
    pub fn new() -> Result<Sensors> {
        let s = System::new_all();
        for component in s.components() {
            println!("{} :{}°C", component.label(), component.temperature());
        }

        Ok(Sensors {
            hostname: hostname()?,
        })
    }
}

fn temps() -> Temperatures {
    Temperatures { cpu: 0.0, gpu: 0.0 }
}

pub fn get_metrics(sensors: &Sensors) -> Result<String> {
    let hostname = &sensors.hostname;
    let mut result = String::with_capacity(256);

    Ok(result)
}
