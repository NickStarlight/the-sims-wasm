//! Recognized Complete Collection routines, compiled into the guest-memory module.
//! Guards leave the original translation available for unusual/faulting inputs.
use super::{
    abi,
    emit_flags::flag_logic,
    emit_helpers::{i32c, i64c, local_get, local_set, reg, set_reg, Seq},
    layout::{A, B, RES, T1, T2, T3},
};
use crate::{guestmem::GuestMem, wasm::opcodes::Op};

pub(super) enum Kernel {
    Compositor565,
    StringHash,
    Geometry,
}

mod compositor;
mod geometry;
mod hash;

impl Kernel {
    pub(super) fn sequence_end(&self, address: u32) -> Option<u32> {
        match self {
            Self::Compositor565 => compositor::sequence_end(address),
            Self::Geometry => geometry::sequence_end(address),
            _ => None,
        }
    }

    pub(super) fn sequence(
        &self,
        instructions: &[&crate::x86::types::Insn],
        context: &super::emit::EmitCtx,
    ) -> Option<super::emit_fpu::runs::Run> {
        match self {
            Self::Compositor565 => compositor::emit(instructions, context),
            Self::Geometry => geometry::emit(instructions, context),
            _ => None,
        }
    }

    pub(super) fn emit(
        &self,
        address: u32,
        context: &super::emit::EmitCtx,
        depth: i32,
    ) -> Option<Seq> {
        match self {
            Self::Compositor565 => compositor::loop_kernel(address, context, depth),
            Self::StringHash => hash::emit(address, context, depth),
            Self::Geometry => None,
        }
    }
}

pub(super) fn recognize(image: &GuestMem, base: u32, address: u32) -> Option<Kernel> {
    let (length, fingerprint, kernel) = match address {
        0x499b54 => (1137, 0x5f0a95f0fb3cf705u64, Kernel::Compositor565),
        0x498830 => (1147, 0xe5767f4600a52773u64, Kernel::Compositor565),
        0x5b8dba => (57, 0x4a457b254a26e704u64, Kernel::StringHash),
        0x434ffd => (53, 0x0c9e770f3074dc7bu64, Kernel::Geometry),
        _ => return None,
    };
    let offset = address.checked_sub(base)? as usize;
    let mut hash = 0xcbf29ce484222325u64;
    for i in 0..length {
        hash = (hash ^ u64::from(image.get(offset + i)?)).wrapping_mul(0x100000001b3);
    }
    (hash == fingerprint).then_some(kernel)
}

fn binary(mut left: Seq, right: Seq, op: u8) -> Seq {
    left.extend(right);
    left.push(op);
    left
}
fn load(address: Seq, op: u8, alignment: u8) -> Seq {
    let mut code = address;
    code.extend([op, alignment, 0]);
    code
}
fn store(address: Seq, value: Seq) -> Seq {
    let mut code = address;
    code.extend(value);
    code.extend([Op::I32_STORE, 2, 0]);
    code
}
fn memory_bytes() -> Seq {
    let mut code = vec![0x3f, 0, Op::I64_EXTEND_I32_U];
    code.extend(i64c(16));
    code.push(Op::I64_SHL);
    code
}
fn within_length(address: Seq, length: Seq) -> Seq {
    let mut code = address;
    code.push(Op::I64_EXTEND_I32_U);
    code.extend(length);
    code.push(Op::I64_ADD);
    code.extend(memory_bytes());
    code.push(0x58); // i64.le_u
    code
}

fn extend(value: Seq) -> Seq {
    let mut code = value;
    code.push(Op::I64_EXTEND_I32_U);
    code
}
fn disjoint(left: Seq, left_length: Seq, right: Seq, right_length: Seq) -> Seq {
    let before = binary(
        binary(extend(left.clone()), extend(left_length), Op::I64_ADD),
        extend(right.clone()),
        0x58,
    );
    let after = binary(
        binary(extend(right), extend(right_length), Op::I64_ADD),
        extend(left),
        0x58,
    );
    binary(before, after, Op::I32_OR)
}

/// Batch only a validated span up to the next original REP checkpoint. A span
/// containing a fault uses the scalar translation, retaining partial progress.
pub(super) fn repeated_store(size: u8) -> Seq {
    let mut code = vec![Op::BLOCK, 0x40];
    let guard = |code: &mut Seq, condition: Seq| {
        code.extend(condition);
        code.extend([Op::I32_EQZ, Op::BR_IF, 0]);
    };
    code.extend(binary(
        reg(abi::ECX),
        i32c(super::scheduling::REP_INTERVAL - 1),
        Op::I32_AND,
    ));
    code.extend(local_set(T1));
    // A zero remainder starts a full 1024-iteration checkpoint interval.
    code.extend(local_get(T1));
    code.extend(i32c(super::scheduling::REP_INTERVAL));
    code.extend(local_get(T1));
    code.extend([Op::SELECT]);
    code.extend(local_set(T1));
    guard(&mut code, binary(local_get(T1), i32c(8), Op::I32_GE_U));
    let backwards = binary(reg(abi::FLAGS), i32c(abi::DF), Op::I32_AND);
    let start = extend(reg(abi::EDI));
    let previous = binary(
        extend(binary(local_get(T1), i32c(1), Op::I32_SUB)),
        i64c(i64::from(size)),
        Op::I64_MUL,
    );
    let low = binary(start.clone(), previous, 0x7d); // i64.sub
    code.extend(low.clone());
    code.extend(start.clone());
    code.extend(backwards.clone());
    code.push(Op::SELECT);
    code.extend(i64c(0));
    code.push(0x59); // i64.ge_s
    guard(&mut code, Vec::new());
    let high_backwards = binary(start.clone(), i64c(i64::from(size)), Op::I64_ADD);
    let high_forwards = binary(
        start,
        binary(extend(local_get(T1)), i64c(i64::from(size)), Op::I64_MUL),
        Op::I64_ADD,
    );
    code.extend(high_backwards);
    code.extend(high_forwards);
    code.extend(backwards.clone());
    code.push(Op::SELECT);
    code.extend(memory_bytes());
    code.push(0x58);
    guard(&mut code, Vec::new());
    code.extend(i32c(-i32::from(size)));
    code.extend(i32c(i32::from(size)));
    code.extend(backwards);
    code.push(Op::SELECT);
    code.extend(local_set(B));
    code.extend(reg(abi::EAX));
    code.extend(local_set(A));
    code.extend(reg(abi::EDI));
    code.extend(local_set(T2));
    code.extend(local_get(T1));
    code.extend(local_set(T3));
    code.extend([Op::LOOP, 0x40]);
    code.extend(local_get(T2));
    code.extend(local_get(A));
    code.extend(match size {
        1 => [Op::I32_STORE8, 0, 0],
        2 => [Op::I32_STORE16, 1, 0],
        _ => [Op::I32_STORE, 2, 0],
    });
    code.extend(binary(local_get(T2), local_get(B), Op::I32_ADD));
    code.extend(local_set(T2));
    code.extend(binary(local_get(T3), i32c(-1), Op::I32_ADD));
    code.extend(local_set(T3));
    code.extend(local_get(T3));
    code.extend([Op::BR_IF, 0, Op::END]);
    code.extend(set_reg(abi::EDI, local_get(T2)));
    code.extend(set_reg(
        abi::ECX,
        binary(reg(abi::ECX), local_get(T1), Op::I32_SUB),
    ));
    code.extend([Op::BR, 1, Op::END]); // continue the original REP loop
    code
}

#[cfg(test)]
#[path = "../../tests/unit/translate/kernels.rs"]
mod tests;
