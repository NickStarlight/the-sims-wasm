//! Original DirectSound vtable layout and supported identities, with process-owned objects.
use super::*;
use crate::modules::{ModuleCatalog, Symbol};
use std::collections::BTreeSet;
pub(super) const INVALID: u32 = 0x80070057;
pub(super) const UNSUPPORTED: u32 = 0x80004001;
pub(super) const CONTROL: u32 = 0x8878001e;
pub(super) const INITIALIZED: u32 = 0x88780082;

pub(super) struct Adapter {
    tables: [u32; 2],
    devices: BTreeSet<u32>,
    objects: BTreeMap<u32, u32>,
    pub priority: u32,
    pub primary: u32,
}
impl Default for Adapter {
    fn default() -> Self {
        Self {
            tables: [0; 2],
            devices: BTreeSet::new(),
            objects: BTreeMap::new(),
            priority: 1,
            primary: 0,
        }
    }
}
impl Audio {
    pub(crate) fn call<M: GuestMemory>(
        &mut self,
        process: &mut Process<M>,
        modules: &ModuleCatalog,
        name: &str,
        args: &[u32],
        now: f64,
    ) -> Result<u32> {
        match name {
            "DirectSoundCreate" | "DirectSoundCreate8" => {
                let object = self.object(process, modules, None)?;
                if args[1] != 0 {
                    put(process, args[1], object)?;
                }
                return Ok(0);
            }
            "DirectSoundEnumerateA" | "DirectSoundEnumerateW" => return Ok(0),
            "DirectSoundCaptureCreate" => return Ok(UNSUPPORTED),
            _ => {}
        }
        let (buffer, slot) = if let Some(slot) = name.strip_prefix("DS_Buffer_") {
            (true, slot)
        } else if let Some(slot) = name.strip_prefix("DS_Device_") {
            (false, slot)
        } else {
            return Err(Error::Host(format!("unknown DirectSound method {name}")));
        };
        let slot: u32 = slot.parse().map_err(|_| Error::InvalidRange)?;
        if buffer {
            let Some(id) = self.adapter.objects.get(&args[0]).copied() else {
                return Ok(INVALID);
            };
            if slot == 0 {
                return query(process, args, true);
            }
            self.buffer_call(process, id, slot, args, now)
        } else {
            if !self.adapter.devices.contains(&args[0]) {
                return Ok(INVALID);
            }
            if slot == 0 {
                return query(process, args, false);
            }
            self.device_call(process, modules, slot, args)
        }
    }
    pub(super) fn object<M: GuestMemory>(
        &mut self,
        process: &mut Process<M>,
        modules: &ModuleCatalog,
        buffer: Option<u32>,
    ) -> Result<u32> {
        let index = usize::from(buffer.is_some());
        let mut table = self.adapter.tables[index];
        if table == 0 {
            let module = modules
                .handle(Some("dsound"))
                .ok_or_else(|| Error::Host("DirectSound native module is not installed".into()))?;
            let mut bytes = Vec::new();
            for slot in 0..if index == 0 { 12 } else { 24 } {
                let name = format!("DS_{}_{slot}", if index == 0 { "Device" } else { "Buffer" });
                let address = modules
                    .procedure(module, Symbol::Name(name.clone()))
                    .map_err(|_| Error::Host(format!("missing DirectSound thunk {name}")))?;
                bytes.extend(address.to_le_bytes());
            }
            table = process.heap_alloc(bytes.len() as u32)?;
            process.write(table, &bytes)?;
            self.adapter.tables[index] = table;
        }
        let object = process.heap_alloc(4)?;
        put(process, object, table)?;
        if let Some(id) = buffer {
            self.adapter.objects.insert(object, id);
        } else {
            self.adapter.devices.insert(object);
        }
        Ok(object)
    }
    pub(super) fn buffer_id(&self, object: u32) -> Option<u32> {
        self.adapter.objects.get(&object).copied()
    }
}
fn query<M: GuestMemory>(process: &mut Process<M>, args: &[u32], buffer: bool) -> Result<u32> {
    if args[2] == 0 {
        return Ok(0x80004003);
    }
    put(process, args[2], 0)?;
    if args[1] == 0 {
        return Ok(INVALID);
    }
    let mut bytes = [0; 16];
    process.read(args[1], &mut bytes)?;
    let actual: [u32; 4] =
        std::array::from_fn(|i| u32::from_le_bytes(bytes[i * 4..i * 4 + 4].try_into().unwrap()));
    let ids = if buffer {
        [
            [0, 0, 0xc0, 0x46000000],
            [0x279afa85, 0x11ce4981, 0x200021a5, 0x60e50baf],
            [0x6825a449, 0x4d827524, 0xe3500f92, 0x1eabb36a],
        ]
    } else {
        [
            [0, 0, 0xc0, 0x46000000],
            [0x279afa83, 0x11ce4981, 0x200021a5, 0x60e50baf],
            [0xc50a7e93, 0x4834f395, 0xa97ff69e, 0x6609e59d],
        ]
    };
    if !ids.contains(&actual) {
        return Ok(0x80004002);
    }
    put(process, args[2], args[0])?;
    Ok(0)
}
pub(super) fn get<M: GuestMemory>(process: &Process<M>, address: u32) -> Result<u32> {
    let mut bytes = [0; 4];
    process.read(address, &mut bytes)?;
    Ok(u32::from_le_bytes(bytes))
}
pub(super) fn put<M: GuestMemory>(
    process: &mut Process<M>,
    address: u32,
    value: u32,
) -> Result<()> {
    process.write(address, &value.to_le_bytes())
}
