//! Compatibility helpers backed by the module encoder's LEB128 implementation.
use wasm_encoder::Encode;

pub fn uleb(value: u32) -> Vec<u8> {
    let mut bytes = Vec::new();
    value.encode(&mut bytes);
    bytes
}

pub fn sleb(value: i32) -> Vec<u8> {
    let mut bytes = Vec::new();
    value.encode(&mut bytes);
    bytes
}
