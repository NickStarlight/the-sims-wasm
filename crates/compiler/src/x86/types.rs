//! Decoded instruction, operand and error types.

pub type Reg = u8; // 0..7
pub type MemSize = u8; // 1 | 2 | 4

#[derive(Clone, Debug, PartialEq)]
pub struct MemOp {
    pub base: i8,  // Reg | -1
    pub index: i8, // Reg | -1
    pub scale: u8, // 1|2|4|8
    pub disp: i32,
    pub size: MemSize,
    pub fs: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Operand {
    Reg { r: Reg, size: MemSize },
    Mem(MemOp),
    Imm(i32),
    St(i32),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
pub enum FlagFx {
    None,
    Logic,
    Arith,
    IncDec,
    Shift,
    Rot,
    Mul,
    Div,
    Neg,
    Fpu,
}

/// Order = golden MNEMS list order. Do NOT reorder.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[repr(u8)]
pub enum Mnem {
    Mov = 0,
    Movsx,
    Movzx,
    Lea,
    Push,
    Pop,
    Add,
    Or,
    Adc,
    Sbb,
    And,
    Sub,
    Xor,
    Cmp,
    Test,
    Xchg,
    Inc,
    Dec,
    Neg,
    Not,
    Mul,
    Imul,
    Div,
    Idiv,
    Pushad,
    Popad,
    Call,
    Jmp,
    Ret,
    Leave,
    Jcc,
    Setcc,
    Jcxz,
    Nop,
    Int3,
    Cwde,
    Cdq,
    Pushf,
    Popf,
    Lahf,
    Sahf,
    Clc,
    Stc,
    Cmc,
    Cld,
    Std,
    Shl,
    Shr,
    Sar,
    Rol,
    Ror,
    Rcl,
    Rcr,
    Shld,
    Shrd,
    Bt,
    Bts,
    Btr,
    Btc,
    Bsf,
    Bsr,
    Bswap,
    Xadd,
    Cmpxchg,
    Cpuid,
    Rdtsc,
    Ud2,
    Movs,
    Stos,
    Lods,
    Scas,
    Cmps,
    Fpu,
    Xlat,
}

/// Order = golden FLAGFX list order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[repr(u8)]
pub enum Rep {
    None = 0,
    WhileEqual = 1, // "rep"/"repe"
    WhileNotEqual = 2,
}

#[derive(Clone, Debug)]
pub struct Insn {
    pub va: u32,
    pub size: u32,
    pub mnem: Mnem,
    pub ops: Vec<Operand>,
    pub cc: Option<u8>,
    pub rel: Option<u32>,
    pub rep: Rep,
    pub flags: FlagFx,
    /// static case targets of an indirect jmp through a switch jump table
    pub switch_targets: Vec<u32>,
}

impl Default for Insn {
    fn default() -> Self {
        Insn {
            va: 0,
            size: 0,
            mnem: Mnem::Nop,
            ops: Vec::new(),
            cc: None,
            rel: None,
            rep: Rep::None,
            flags: FlagFx::None,
            switch_targets: Vec::new(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct DecodeError {
    pub msg: &'static str,
    pub va: u32,
    pub byte: u8,
}

impl DecodeError {
    pub fn new(msg: &'static str, va: u32, byte: Option<u8>) -> Self {
        DecodeError {
            msg,
            va,
            byte: byte.unwrap_or(0),
        }
    }
}

pub type DResult<T> = Result<T, DecodeError>;

impl std::fmt::Display for DecodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} at {:#x} (opcode {:#x})",
            self.msg, self.va, self.byte
        )
    }
}
