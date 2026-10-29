//! Translation inputs and diagnostic output metadata.

use crate::pe::PeInfo;
use crate::x86::types::Mnem;

/// A second guest image (a mapped DLL) the translator compiles from
/// The module's PeInfo holds preferred-base virtual addresses.
pub struct ModuleImage<'a> {
    pub base: u32,
    pub size_of_image: u32,
    pub info: &'a PeInfo,
}

pub struct Program {
    pub diagnostics: Vec<String>,
    pub wasm: Vec<u8>,
    /// Sorted guest function addresses (table slot i+1 ↔ funcAddrs[i]).
    pub func_addrs: Vec<u32>,
    pub table_addr: u32,
    pub table_func_count: u32,
    pub entry: u32,
    /// First table slot of the trampoline pool.
    pub tramp_base: u32,
    /// Absolute module offset of each translated function's body.
    pub func_body_starts: Vec<usize>,
    /// Per translated function, the sorted guest VAs of every dispatch-segment start.
    pub blocks_by_func: Vec<(u32, Vec<u32>)>,
    /// Per-function instruction maps (wasmOff relative to body start + locals decl).
    pub insn_maps: Vec<(u32, Vec<InsnMapEntry>)>,
}

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InsnMapEntry {
    pub wasm_off: u32,
    pub guest_va: u32,
    pub mnem: Mnem,
    pub flags: crate::x86::types::FlagFx,
    pub rel: Option<i32>,
}
