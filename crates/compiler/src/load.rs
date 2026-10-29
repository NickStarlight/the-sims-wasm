//! Coarse image-loading boundary: image reservations plus packed initialized bytes.
use crate::{guestmem::GuestMem, ModuleMetadata};
use serde::Serialize;

#[derive(Default, Serialize)]
pub struct LoadPlan {
    pub images: Vec<ModuleMetadata>,
    pub spans: Vec<LoadSpan>,
    #[serde(skip)]
    pub bytes: Vec<u8>,
}

#[derive(Serialize)]
pub struct LoadSpan {
    pub address: u32,
    pub offset: usize,
    pub length: u32,
}

impl LoadPlan {
    pub(crate) fn append(&mut self, memory: &GuestMem, address: u32, length: u32) {
        if length == 0 {
            return;
        }
        self.spans.push(LoadSpan {
            address,
            offset: self.bytes.len(),
            length,
        });
        self.bytes.extend((0..length).map(|offset| {
            memory
                .get((address + offset) as usize)
                .expect("validated image range")
        }));
    }

    pub fn metadata_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }
}
