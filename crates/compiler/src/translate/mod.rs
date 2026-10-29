//! Native x86 discovery and Wasm code generation.

pub mod blocks;
pub mod cond;
pub mod emit;
pub mod emit_flags;
pub mod emit_fpu;
pub mod emit_helpers;
pub mod error;
mod kernels;
pub mod layout;
mod optimize;
pub mod program;
mod scheduling;

/// ABI constants — mirrors src/abi.ts + src/mem/layout.ts + program.ts constants.
pub mod abi {
    pub const REG_GLOBALS: [&str; 16] = [
        "eax",
        "ecx",
        "edx",
        "ebx",
        "esp",
        "ebp",
        "esi",
        "edi",
        "flags",
        "fp_top",
        "fp_status",
        "trap_va",
        "call_target",
        "resume_next",
        "actual_return",
        "teb_base",
    ];
    pub const EAX: u8 = 0;
    pub const ECX: u8 = 1;
    pub const EDX: u8 = 2;
    pub const EBX: u8 = 3;
    pub const ESP: u8 = 4;
    pub const EBP: u8 = 5;
    pub const ESI: u8 = 6;
    pub const EDI: u8 = 7;
    pub const FLAGS: u8 = 8;
    pub const FP_TOP: u8 = 9;
    pub const FP_STATUS: u8 = 10;
    pub const TRAP_VA: u8 = 11;
    pub const CALL_TARGET: u8 = 12;
    pub const RESUME_NEXT: u8 = 13;
    pub const ACTUAL_RETURN: u8 = 14;

    /// EFLAGS bits (x86 layout).
    pub const CF: i32 = 0x0000_0001;
    pub const PF: i32 = 0x0000_0004;
    pub const ZF: i32 = 0x0000_0040;
    pub const SF: i32 = 0x0000_0080;
    pub const DF: i32 = 0x0000_0400;
    pub const OF: i32 = 0x0000_0800;

    pub const TEB_BASE: u8 = 15;
    pub const FP_OFFSET: i32 = 0x400;
    // Private per-instance global after the imported registers and checkpoint budget.
    pub const FP_CONTROL: u8 = 17;

    pub const API_BASE: u32 = 0x0080_0000;
    pub const API_END: u32 = 0x0090_0000; // program.ts local constant
    pub const TAB_ADDR: u32 = 0x00a0_0000; // program.ts local constant
    pub const COMMAND_LINE_ADDR: u32 = 0x00d0_0000;

    /// FPU helper ids (src/runtime/api-space.ts FPU_HELPER) — dispatched via the
    /// dispatch import (wasm function index 0).
    pub mod fpu_helper {
        pub const FPATAN: i32 = 1;
        pub const FPTAN: i32 = 2;
        pub const FYL2X: i32 = 3;
        pub const F2XM1: i32 = 4;
        pub const FYL2XP1: i32 = 5;
        pub const FSIN: i32 = 6;
        pub const FCOS: i32 = 7;
        pub const FSINCOS: i32 = 8;
        pub const RDTSC: i32 = 9;
        pub const CPUID: i32 = 10;
    }
}

#[cfg(test)]
#[path = "../../tests/support/translate.rs"]
mod test_support;
