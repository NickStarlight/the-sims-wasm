//! Supported mnemonic and flag classifications at the compiler boundary.
use super::types::{FlagFx, Mnem};
use iced_x86::{Instruction, Mnemonic as X};

pub(super) fn mnemonic(insn: &Instruction) -> Option<Mnem> {
    Some(match insn.mnemonic() {
        X::Mov => Mnem::Mov,
        X::Movsx => Mnem::Movsx,
        X::Movzx => Mnem::Movzx,
        X::Lea => Mnem::Lea,
        X::Push => Mnem::Push,
        X::Pop => Mnem::Pop,
        X::Add => Mnem::Add,
        X::Or => Mnem::Or,
        X::Adc => Mnem::Adc,
        X::Sbb => Mnem::Sbb,
        X::And => Mnem::And,
        X::Sub => Mnem::Sub,
        X::Xor => Mnem::Xor,
        X::Cmp => Mnem::Cmp,
        X::Test => Mnem::Test,
        X::Xchg => Mnem::Xchg,
        X::Inc => Mnem::Inc,
        X::Dec => Mnem::Dec,
        X::Neg => Mnem::Neg,
        X::Not => Mnem::Not,
        X::Mul => Mnem::Mul,
        X::Imul => Mnem::Imul,
        X::Div => Mnem::Div,
        X::Idiv => Mnem::Idiv,
        X::Call => Mnem::Call,
        X::Jmp => Mnem::Jmp,
        X::Ret => Mnem::Ret,
        X::Leave => Mnem::Leave,
        X::Nop => Mnem::Nop,
        X::Int3 => Mnem::Int3,
        X::Cwde => Mnem::Cwde,
        X::Cdq => Mnem::Cdq,
        X::Lahf => Mnem::Lahf,
        X::Sahf => Mnem::Sahf,
        X::Clc => Mnem::Clc,
        X::Stc => Mnem::Stc,
        X::Cmc => Mnem::Cmc,
        X::Cld => Mnem::Cld,
        X::Std => Mnem::Std,
        X::Shl => Mnem::Shl,
        X::Shr => Mnem::Shr,
        X::Sar => Mnem::Sar,
        X::Rol => Mnem::Rol,
        X::Ror => Mnem::Ror,
        X::Rcl => Mnem::Rcl,
        X::Rcr => Mnem::Rcr,
        X::Shld => Mnem::Shld,
        X::Shrd => Mnem::Shrd,
        X::Bt => Mnem::Bt,
        X::Bts => Mnem::Bts,
        X::Btr => Mnem::Btr,
        X::Btc => Mnem::Btc,
        X::Bsf => Mnem::Bsf,
        X::Bsr => Mnem::Bsr,
        X::Bswap => Mnem::Bswap,
        X::Xadd => Mnem::Xadd,
        X::Cmpxchg => Mnem::Cmpxchg,
        X::Cpuid => Mnem::Cpuid,
        X::Rdtsc => Mnem::Rdtsc,
        X::Ud2 => Mnem::Ud2,
        X::Pushad => Mnem::Pushad,
        X::Popad => Mnem::Popad,
        X::Pushf | X::Pushfd => Mnem::Pushf,
        X::Popf | X::Popfd => Mnem::Popf,
        X::Jecxz => Mnem::Jcxz,
        X::Jo
        | X::Jno
        | X::Jb
        | X::Jae
        | X::Je
        | X::Jne
        | X::Jbe
        | X::Ja
        | X::Js
        | X::Jns
        | X::Jp
        | X::Jnp
        | X::Jl
        | X::Jge
        | X::Jle
        | X::Jg => Mnem::Jcc,
        X::Seto
        | X::Setno
        | X::Setb
        | X::Setae
        | X::Sete
        | X::Setne
        | X::Setbe
        | X::Seta
        | X::Sets
        | X::Setns
        | X::Setp
        | X::Setnp
        | X::Setl
        | X::Setge
        | X::Setle
        | X::Setg => Mnem::Setcc,
        X::Movsb | X::Movsw | X::Movsd if insn.op0_kind() == iced_x86::OpKind::MemoryESEDI => {
            Mnem::Movs
        }
        X::Stosb | X::Stosw | X::Stosd if insn.op0_kind() == iced_x86::OpKind::MemoryESEDI => {
            Mnem::Stos
        }
        X::Lodsb | X::Lodsw | X::Lodsd if insn.op1_kind() == iced_x86::OpKind::MemorySegESI => {
            Mnem::Lods
        }
        X::Scasb | X::Scasw | X::Scasd if insn.op1_kind() == iced_x86::OpKind::MemoryESEDI => {
            Mnem::Scas
        }
        X::Cmpsb | X::Cmpsw | X::Cmpsd if insn.op0_kind() == iced_x86::OpKind::MemorySegESI => {
            Mnem::Cmps
        }
        X::Xlatb => Mnem::Xlat,
        X::Int => Mnem::Int3,
        X::Wait | X::Cli | X::Sti => Mnem::Nop,
        X::Sal => Mnem::Shl,
        _ => return None,
    })
}

pub(super) fn string_width(mnemonic: X) -> Option<i32> {
    Some(match mnemonic {
        X::Movsb | X::Stosb | X::Lodsb | X::Scasb | X::Cmpsb => 1,
        X::Movsw | X::Stosw | X::Lodsw | X::Scasw | X::Cmpsw => 2,
        X::Movsd | X::Stosd | X::Lodsd | X::Scasd | X::Cmpsd => 4,
        _ => return None,
    })
}

pub(super) fn flags(mnemonic: Mnem) -> FlagFx {
    match mnemonic {
        Mnem::Add | Mnem::Adc | Mnem::Sub | Mnem::Sbb | Mnem::Cmp => FlagFx::Arith,
        Mnem::And
        | Mnem::Or
        | Mnem::Xor
        | Mnem::Test
        | Mnem::Bt
        | Mnem::Bts
        | Mnem::Btr
        | Mnem::Btc
        | Mnem::Bsf
        | Mnem::Bsr => FlagFx::Logic,
        Mnem::Inc | Mnem::Dec => FlagFx::IncDec,
        Mnem::Shl | Mnem::Shr | Mnem::Sar | Mnem::Shld | Mnem::Shrd => FlagFx::Shift,
        Mnem::Rol | Mnem::Ror | Mnem::Rcl | Mnem::Rcr => FlagFx::Rot,
        Mnem::Mul | Mnem::Imul => FlagFx::Mul,
        Mnem::Div | Mnem::Idiv => FlagFx::Div,
        Mnem::Neg => FlagFx::Neg,
        Mnem::Fpu => FlagFx::Fpu,
        _ => FlagFx::None,
    }
}
