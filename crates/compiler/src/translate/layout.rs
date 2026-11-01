//! Local slot layout for every translated function — mirrors src/translate/layout.ts.

pub const NEXT: u32 = 0; // i32: block dispatch id
pub const A: u32 = 1; // i32: flag operand a
pub const B: u32 = 2; // i32: flag operand b
pub const RES: u32 = 3; // i32: flag result
pub const T1: u32 = 9; // i32 scratch (address)
pub const T2: u32 = 10; // i32 scratch (result stash)
pub const T3: u32 = 11; // i32 scratch
                        // Local 12 is reserved for compatibility with the existing scratch layout.
pub const FULL: u32 = 13; // i64 scratch (mul/rot math)
pub const ENTRY_SP: u32 = 14; // i32: native frame boundary for nonlocal returns
pub const FTMP: u32 = 28; // f64 scratch (x87 slot swap)
pub const CACHED_REGISTERS: u32 = 29;
pub const FP_LOCALS: u32 = 40; // f64: x87 slots relative to the run's initial TOP

/// wasm local declarations: [count, type] runs — 0x7f i32, 0x7e i64, 0x7c f64.
pub const LOCAL_RUNS: [(u32, u8); 6] = [
    (13, 0x7f),
    (1, 0x7e),
    (14, 0x7f),
    (1, 0x7c),
    (11, 0x7f),
    (8, 0x7c),
];

/// Expanded local types list (the FuncDef.locals input).
pub fn locals_list() -> Vec<u8> {
    let mut v = Vec::new();
    for (count, ty) in LOCAL_RUNS {
        for _ in 0..count {
            v.push(ty);
        }
    }
    v
}
