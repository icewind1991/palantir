use crate::sensors::DiskUsage;
use color_eyre::Result;
use std::process::Command;

pub fn pools() -> impl Iterator<Item = DiskUsage> {
    OutputParser {
        str: zpool_command().unwrap_or_default(),
        pos: 0,
    }
}

fn zpool_command() -> Result<String> {
    let mut z = Command::new("zpool");
    z.args(&["list", "-p", "-H", "-o", "name,size,free"]);
    let out = z.output()?;
    if out.status.success() {
        Ok(String::from_utf8(out.stdout)?)
    } else {
        Ok(String::new())
    }
}

fn parse_line(line: &str) -> Option<DiskUsage> {
    let mut parts = line.split_ascii_whitespace();
    let name = parts.next()?.to_string();
    let size = parts.next()?.parse().ok()?;
    let free = parts.next()?.parse().ok()?;
    Some(DiskUsage { name, size, free })
}

struct OutputParser {
    str: String,
    pos: usize,
}

impl Iterator for OutputParser {
    type Item = DiskUsage;

    fn next(&mut self) -> Option<Self::Item> {
        let str = self.str.as_str();
        let line = match str[self.pos..].find('\n') {
            Some(next_pos) => {
                let old_pos = self.pos;
                self.pos += next_pos + 1;
                Some(&str[old_pos..self.pos])
            }
            None if self.pos < str.len() => {
                let old_pos = self.pos;
                self.pos = str.len();
                Some(&str[old_pos..])
            }
            None => None,
        };
        line.and_then(parse_line)
    }
}
