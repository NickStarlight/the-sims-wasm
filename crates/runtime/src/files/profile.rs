//! INI reads and writes operate on the same process-local file overlays.
use super::*;
type Sections = BTreeMap<String, BTreeMap<String, String>>;
fn parse(bytes: &[u8]) -> Sections {
    let text: String = bytes.iter().copied().map(char::from).collect();
    let mut sections = Sections::new();
    let mut section = String::new();
    for line in text.lines().map(str::trim) {
        if line.starts_with([';', '#']) || line.is_empty() {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            section = line[1..line.len() - 1].trim().to_lowercase();
            sections.entry(section.clone()).or_default();
        } else if let Some((key, value)) = line.split_once('=') {
            sections.entry(section.clone()).or_default().insert(
                key.trim().to_lowercase(),
                value.trim().trim_matches('"').to_owned(),
            );
        }
    }
    sections
}
impl Files {
    pub(super) fn profile<M: GuestMemory>(
        &mut self,
        process: &mut Process<M>,
        name: &str,
        args: &[u32],
    ) -> Result<(u32, u32)> {
        let filename = if name == "GetPrivateProfileStringA" {
            args[5]
        } else {
            args[3]
        };
        let spelling = self.cased_path(process, filename)?;
        let path = spelling.to_lowercase();
        let mut sections = self
            .entries
            .get(&path)
            .map(|node| node.borrow().bytes().map(|bytes| parse(&bytes)))
            .transpose()?
            .unwrap_or_default();
        let section = read_string(process, args[0])?.to_lowercase();
        let key = read_string(process, args[1])?.to_lowercase();
        match name {
            "GetPrivateProfileIntA" => {
                let value = sections.get(&section).and_then(|s| s.get(&key));
                let parsed = value.and_then(|value| {
                    if let Some(hex) = value
                        .strip_prefix("0x")
                        .or_else(|| value.strip_prefix("0X"))
                    {
                        u32::from_str_radix(hex, 16).ok()
                    } else {
                        value.parse::<i32>().ok().map(|v| v as u32)
                    }
                });
                Ok((parsed.unwrap_or(args[2]), 0))
            }
            "GetPrivateProfileStringA" => {
                if args[0] == 0 || args[1] == 0 {
                    let names: Vec<_> = if args[0] == 0 {
                        sections.keys().cloned().collect()
                    } else {
                        sections
                            .get(&section)
                            .map(|s| s.keys().cloned().collect())
                            .unwrap_or_default()
                    };
                    let empty = names.is_empty();
                    let mut bytes = names.join("\0").into_bytes();
                    bytes.extend([0, 0]);
                    let capacity = args[4] as usize;
                    if capacity == 0 {
                        return Ok((0, 0));
                    }
                    let truncated = bytes.len() > capacity;
                    bytes.truncate(capacity);
                    let length = bytes.len();
                    bytes[length - 1] = 0;
                    if truncated && length > 1 {
                        bytes[length - 2] = 0;
                    }
                    process.write(args[3], &bytes)?;
                    return Ok((
                        if empty {
                            0
                        } else if truncated {
                            capacity.saturating_sub(2)
                        } else {
                            length.saturating_sub(1)
                        } as u32,
                        0,
                    ));
                }
                let default = read_string(process, args[2])?;
                let value = sections
                    .get(&section)
                    .and_then(|s| s.get(&key))
                    .map_or(default.trim_end(), String::as_str);
                Ok((output_string(process, args[3], args[4], value)?, 0))
            }
            "WritePrivateProfileStringA" => {
                if args[0] == 0 && args[1] == 0 && args[2] == 0 {
                    return Ok((1, 0));
                }
                if args[0] == 0 {
                    return Ok((0, 87));
                }
                if args[1] == 0 {
                    sections.remove(&section);
                } else if args[2] == 0 {
                    if let Some(values) = sections.get_mut(&section) {
                        values.remove(&key);
                    }
                } else {
                    sections
                        .entry(section)
                        .or_default()
                        .insert(key, read_string(process, args[2])?);
                }
                let mut bytes = String::new();
                for (section, values) in sections {
                    bytes.push_str(&format!("[{section}]\r\n"));
                    for (key, value) in values {
                        bytes.push_str(&format!("{key}={value}\r\n"));
                    }
                }
                if !self.directories.contains(paths::parent(&path)) {
                    return Ok((0, 3));
                }
                self.record(
                    journal::Change::Replace {
                        path: spelling.clone(),
                    },
                    bytes.as_bytes(),
                )?;
                self.remember_name(&spelling);
                let node = self.entries.entry(path).or_insert_with(|| {
                    Rc::new(RefCell::new(Node {
                        original: Arc::from([]),
                        source: None,
                        overlay: None,
                    }))
                });
                node.borrow_mut().overlay = Some(bytes.into_bytes());
                Ok((1, 0))
            }
            _ => Err(Error::Host(format!("unknown profile API {name}"))),
        }
    }
}
