//! The audio-only FilterGraph interfaces used by the original MP3 player.
//! Decoding is packet-bounded; completion returns through the guest's event window.
use super::*;
use crate::audio::{music::MusicStream, StereoBlock};
use crate::modules::Symbol;
use std::collections::BTreeMap;

const INVALID: u32 = 0x80070057;
const POINTER: u32 = 0x80004003;
const NO_INTERFACE: u32 = 0x80004002;
const UNKNOWN: [u8; 16] = [0, 0, 0, 0, 0, 0, 0, 0, 0xc0, 0, 0, 0, 0, 0, 0, 0x46];
const FILTER_GRAPH: [u8; 16] = [
    0xb3, 0xeb, 0x36, 0xe4, 0x4f, 0x52, 0xce, 0x11, 0x9f, 0x53, 0, 0x20, 0xaf, 0x0b, 0xa7, 0x70,
];
fn iid(first: u32) -> [u8; 16] {
    let mut bytes = [
        0, 0, 0, 0, 0xd4, 0x0a, 0xce, 0x11, 0xb0, 0x3a, 0, 0x20, 0xaf, 0x0b, 0xa7, 0x70,
    ];
    bytes[..4].copy_from_slice(&first.to_le_bytes());
    bytes
}
fn guid<M: GuestMemory>(p: &Process<M>, address: u32) -> Result<[u8; 16]> {
    let mut bytes = [0; 16];
    if address != 0 {
        p.read(address, &mut bytes)?;
    }
    Ok(bytes)
}
fn put<M: GuestMemory>(p: &mut Process<M>, address: u32, value: u32) -> Result<()> {
    p.write(address, &value.to_le_bytes())
}
#[derive(Default)]
pub(super) struct DirectShow {
    graphs: BTreeMap<u32, Graph>,
    objects: BTreeMap<u32, (u32, usize)>,
    tables: [u32; 5],
}
struct Graph {
    interfaces: [u32; 5],
    references: u32,
    stream: Option<MusicStream>,
    state: u32,
    volume: i32,
    balance: i32,
    notification: [u32; 3],
    notify_flags: u32,
    complete: bool,
    end_notified: bool,
    error: Option<u32>,
}
impl DirectShow {
    pub(super) fn mix(
        &mut self,
        block: &mut StereoBlock,
        rate: u32,
        user: &mut crate::user::User,
    ) -> Result<()> {
        for graph in self.graphs.values_mut() {
            if graph.state != 2 {
                continue;
            }
            let Some(stream) = graph.stream.as_mut() else {
                continue;
            };
            let gain = |db: i32| {
                if db <= -10000 {
                    0.0
                } else {
                    libm::powf(10.0, db as f32 / 2000.0)
                }
            };
            let result = stream.mix(
                &mut block.left,
                &mut block.right,
                rate,
                gain(graph.volume) * gain(-graph.balance.max(0)),
                gain(graph.volume) * gain(graph.balance.min(0)),
            );
            let notify = if result.is_err() {
                // A corrupt track must not break the worklet request/reply loop
                // or silence other graphs. Report the failure through COM.
                graph.stream = None;
                graph.state = 0;
                graph.error = Some(0x80004005); // E_FAIL
                true
            } else if stream.finished() && !graph.end_notified {
                // Completion does not stop a DirectShow graph. Keep its running
                // state, while posting this EOF only once per run transition.
                graph.complete = true;
                graph.end_notified = true;
                true
            } else {
                false
            };
            if notify {
                let [window, message, data] = graph.notification;
                if window != 0 && graph.notify_flags == 0 {
                    user.post_notification(window, message, data)?;
                }
            }
        }
        Ok(())
    }
}
impl Kernel {
    pub(super) fn create_graph<M: GuestMemory>(
        &mut self,
        p: &mut Process<M>,
        a: &[u32],
    ) -> Result<u32> {
        if a[4] == 0 {
            return Ok(POINTER);
        }
        put(p, a[4], 0)?;
        if guid(p, a[0])? != FILTER_GRAPH {
            return Ok(0x80040154);
        }
        if a[1] != 0 {
            return Ok(0x80040110);
        }
        let requested = guid(p, a[3])?;
        let Some(kind) = interface_kind(requested) else {
            return Ok(NO_INTERFACE);
        };
        if kind == 4 {
            return Ok(NO_INTERFACE);
        }
        let specs: [&[(&str, u32)]; 5] = [
            &[
                ("Query", 3),
                ("AddRef", 1),
                ("Release", 1),
                ("", 3),
                ("", 2),
                ("Enum", 2),
                ("", 3),
                ("", 4),
                ("", 2),
                ("", 2),
                ("", 1),
                ("", 3),
                ("", 2),
                ("Render", 3),
                ("", 4),
                ("", 2),
                ("", 1),
                ("", 1),
            ],
            &[
                ("Query", 3),
                ("AddRef", 1),
                ("Release", 1),
                ("", 2),
                ("", 4),
                ("", 6),
                ("", 9),
                ("Run", 1),
                ("Pause", 1),
                ("Stop", 1),
                ("State", 3),
                ("", 2),
                ("", 2),
                ("", 2),
                ("", 2),
                ("", 1),
            ],
            &[
                ("Query", 3),
                ("AddRef", 1),
                ("Release", 1),
                ("", 2),
                ("", 4),
                ("", 6),
                ("", 9),
                ("", 2),
                ("Event", 5),
                ("", 3),
                ("", 2),
                ("", 2),
                ("FreeEvent", 4),
                ("Notify", 4),
                ("Flags", 2),
                ("GetFlags", 2),
            ],
            &[
                ("Query", 3),
                ("AddRef", 1),
                ("Release", 1),
                ("", 2),
                ("", 4),
                ("", 6),
                ("", 9),
                ("Volume", 2),
                ("GetVolume", 2),
                ("Balance", 2),
                ("GetBalance", 2),
            ],
            &[
                ("Query", 3),
                ("AddRef", 1),
                ("Release", 1),
                ("Next", 4),
                ("Skip", 2),
                ("Reset", 1),
                ("Clone", 2),
            ],
        ];
        let module = self
            .modules
            .handle(Some("quartz"))
            .ok_or_else(|| Error::Host("DirectShow native module is not installed".into()))?;
        for (kind, spec) in specs.iter().enumerate() {
            if self.directshow.tables[kind] != 0 {
                continue;
            }
            let mut bytes = Vec::new();
            for &(name, arity) in *spec {
                let name = if name.is_empty() {
                    format!("DG_Unsupported{arity}")
                } else {
                    format!("DG_{name}")
                };
                bytes.extend(
                    self.modules
                        .procedure(module, Symbol::Name(name))
                        .map_err(|_| Error::Host("missing DirectShow thunk".into()))?
                        .to_le_bytes(),
                );
            }
            let table = p.heap_alloc(bytes.len() as u32)?;
            p.write(table, &bytes)?;
            self.directshow.tables[kind] = table;
        }
        let mut interfaces = [0; 5];
        let allocated = (|| -> Result<()> {
            for (kind, address) in interfaces.iter_mut().enumerate() {
                *address = p.heap_alloc(4)?;
                put(p, *address, self.directshow.tables[kind])?;
            }
            put(p, a[4], interfaces[kind])
        })();
        if let Err(error) = allocated {
            for address in interfaces.into_iter().filter(|&address| address != 0) {
                p.heap_free(address)?;
            }
            return Err(error);
        }
        let id = interfaces[0];
        for (kind, address) in interfaces.iter().enumerate() {
            self.directshow.objects.insert(*address, (id, kind));
        }
        self.directshow.graphs.insert(
            id,
            Graph {
                interfaces,
                references: 1,
                stream: None,
                state: 0,
                volume: 0,
                balance: 0,
                notification: [0; 3],
                notify_flags: 0,
                complete: false,
                end_notified: false,
                error: None,
            },
        );
        Ok(0)
    }
    pub(super) fn graph_call<M: GuestMemory>(
        &mut self,
        p: &mut Process<M>,
        name: &str,
        a: &[u32],
    ) -> Result<u32> {
        let Some(&(id, kind)) = self.directshow.objects.get(&a[0]) else {
            return Ok(INVALID);
        };
        let graph = self.directshow.graphs.get_mut(&id).unwrap();
        match name {
            "DG_Query" => {
                if a[2] == 0 {
                    return Ok(POINTER);
                }
                put(p, a[2], 0)?;
                let requested = guid(p, a[1])?;
                let target = if kind == 4 {
                    if requested == UNKNOWN || requested == iid(0x56a86893) {
                        Some(4)
                    } else {
                        None
                    }
                } else {
                    interface_kind(requested).filter(|&x| x != 4)
                };
                let Some(target) = target else {
                    return Ok(NO_INTERFACE);
                };
                put(p, a[2], graph.interfaces[target])?;
                graph.references += 1;
                Ok(0)
            }
            "DG_AddRef" => {
                graph.references += 1;
                Ok(graph.references)
            }
            "DG_Release" => {
                graph.references -= 1;
                let refs = graph.references;
                if refs == 0 {
                    let graph = self.directshow.graphs.remove(&id).unwrap();
                    for address in graph.interfaces {
                        self.directshow.objects.remove(&address);
                        p.heap_free(address)?;
                    }
                }
                Ok(refs)
            }
            "DG_Enum" | "DG_Clone" => {
                if a[1] == 0 {
                    return Ok(POINTER);
                }
                put(p, a[1], graph.interfaces[4])?;
                graph.references += 1;
                Ok(0)
            }
            "DG_Next" => {
                if a[3] != 0 {
                    put(p, a[3], 0)?;
                }
                Ok(if a[1] == 0 { 0 } else { 1 })
            }
            "DG_Skip" => Ok(if a[1] == 0 { 0 } else { 1 }),
            "DG_Reset" | "DG_FreeEvent" => Ok(0),
            "DG_Render" => {
                if a[1] == 0 || a[2] != 0 {
                    return Ok(INVALID);
                }
                let mut wide = Vec::new();
                for offset in 0..32768u32 {
                    let mut bytes = [0; 2];
                    p.read(
                        a[1].checked_add(offset * 2).ok_or(Error::InvalidRange)?,
                        &mut bytes,
                    )?;
                    let unit = u16::from_le_bytes(bytes);
                    if unit == 0 {
                        break;
                    }
                    wide.push(unit);
                }
                let path = String::from_utf16_lossy(&wide);
                let Some(bytes) = self.files.file_bytes(&path)? else {
                    return Ok(0x80070002);
                };
                match MusicStream::new(bytes) {
                    Ok(stream) => {
                        graph.stream = Some(stream);
                        graph.state = 0;
                        graph.complete = false;
                        graph.end_notified = false;
                        graph.error = None;
                        Ok(0)
                    }
                    Err(_) => Ok(0x80040265),
                }
            }
            "DG_Run" | "DG_Pause" => {
                if graph.stream.is_none() {
                    return Ok(0x80040209);
                }
                if name == "DG_Run" && graph.state != 2 {
                    graph.end_notified = false;
                }
                graph.state = if name == "DG_Run" { 2 } else { 1 };
                Ok(0)
            }
            "DG_Stop" => {
                graph.state = 0;
                graph.end_notified = false;
                Ok(0)
            }
            "DG_State" => {
                if a[2] == 0 {
                    return Ok(POINTER);
                }
                put(p, a[2], graph.state)?;
                Ok(0)
            }
            "DG_Volume" => {
                let value = a[1] as i32;
                if !(-10000..=0).contains(&value) {
                    return Ok(INVALID);
                }
                graph.volume = value;
                Ok(0)
            }
            "DG_Balance" => {
                let value = a[1] as i32;
                if !(-10000..=10000).contains(&value) {
                    return Ok(INVALID);
                }
                graph.balance = value;
                Ok(0)
            }
            "DG_GetVolume" | "DG_GetBalance" => {
                if a[1] == 0 {
                    return Ok(POINTER);
                }
                put(
                    p,
                    a[1],
                    if name == "DG_GetVolume" {
                        graph.volume
                    } else {
                        graph.balance
                    } as u32,
                )?;
                Ok(0)
            }
            "DG_Notify" => {
                if a[1] != 0 && self.user.call(p, "IsWindow", &[a[1]])? == 0 {
                    return Ok(INVALID);
                }
                graph.notification = [a[1], a[2], a[3]];
                Ok(0)
            }
            "DG_Flags" => {
                if a[1] > 1 {
                    return Ok(INVALID);
                }
                graph.notify_flags = a[1];
                Ok(0)
            }
            "DG_GetFlags" => {
                if a[1] == 0 {
                    return Ok(POINTER);
                }
                put(p, a[1], graph.notify_flags)?;
                Ok(0)
            }
            "DG_Event" => {
                if a[1..4].contains(&0) {
                    return Ok(POINTER);
                }
                if !graph.complete && graph.error.is_none() {
                    return Ok(0x80004004);
                }
                put(p, a[1], if graph.error.is_some() { 3 } else { 1 })?;
                put(p, a[2], graph.error.unwrap_or(0))?;
                put(p, a[3], 0)?;
                if graph.error.take().is_none() {
                    graph.complete = false;
                }
                Ok(0)
            }
            _ => Ok(0x80004001),
        }
    }
}
fn interface_kind(id: [u8; 16]) -> Option<usize> {
    if id == UNKNOWN || id == iid(0x56a868a9) || id == iid(0x56a8689f) {
        Some(0)
    } else if id == iid(0x56a868b1) {
        Some(1)
    } else if id == iid(0x56a868c0) || id == iid(0x56a868b6) {
        Some(2)
    } else if id == iid(0x56a868b3) {
        Some(3)
    } else {
        None
    }
}

#[cfg(test)]
#[path = "../../tests/unit/kernel/directshow.rs"]
mod tests;

#[cfg(test)]
#[path = "../../tests/unit/kernel/directshow/behavior.rs"]
mod behavior_tests;
