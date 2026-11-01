//! Forward x87 slots through locals within guarded, uninterrupted arithmetic runs.
//!
//! The emulated x87 bank lives in guest memory, so ordinary operands can alias
//! it. Preflight all access ranges before deferring any bank write or TOP update.
//! Aliases and potential faults take the unchanged instruction-by-instruction
//! path, retaining its partial side effects. The caller must stop at checkpoints.
use super::*;
use crate::translate::layout::{FP_LOCALS, T2, T3};
use crate::x86::types::{Mnem, Operand};

enum Operation<'a> {
    Load(&'a MemOp, bool),
    Store(&'a MemOp, bool, bool),
    MemoryMath(&'a MemOp, bool, u8, bool),
    RegisterMath(i32, i32, i32, u8, bool),
    Copy(i32, i32, bool),
    Push(i32),
    Swap(i32),
    Unary(u8),
    Round,
    Constant(f64),
}

fn decode(insn: &Insn) -> Option<Operation<'_>> {
    if insn.mnem != Mnem::Fpu {
        return None;
    }
    let Operand::Imm(raw) = insn.ops.first()? else {
        return None;
    };
    let raw = *raw as u32;
    let field = (raw >> 3) & 7;
    if raw & 0xc0 != 0xc0 {
        let Operand::Mem(m) = insn.ops.get(1)? else {
            return None;
        };
        let row = raw >> 8;
        return match row {
            0xd9 | 0xdd => match field {
                0 => Some(Operation::Load(m, row == 0xdd)),
                2 | 3 => Some(Operation::Store(m, row == 0xdd, field == 3)),
                _ => None,
            },
            0xd8 | 0xdc => {
                math(field).map(|(op, reverse)| Operation::MemoryMath(m, row == 0xdc, op, reverse))
            }
            _ => None,
        };
    }
    let i = (raw & 7) as i32;
    match raw & 0xfff8 {
        0xd9c0 => return Some(Operation::Push(i)),
        0xd9c8 => return Some(Operation::Swap(i)),
        0xddd0 | 0xddd8 => return Some(Operation::Copy(i, 0, raw & 8 != 0)),
        _ => {}
    }
    let row = raw >> 8;
    if matches!(row, 0xd8 | 0xdc | 0xde) {
        let (op, reverse) = if row == 0xd8 {
            math(field)?
        } else {
            // DC/DE encode the subtract/divide reverse forms in the other order.
            let (op, reverse) = math(field)?;
            (op, field >= 4 && !reverse)
        };
        let destination = if row == 0xd8 { 0 } else { i };
        let (left, right) = if row == 0xd8 { (0, i) } else { (i, 0) };
        let (left, right) = if reverse {
            (right, left)
        } else {
            (left, right)
        };
        return Some(Operation::RegisterMath(
            destination,
            left,
            right,
            op,
            row == 0xde,
        ));
    }
    Some(match raw {
        0xd9e0 => Operation::Unary(Op::F64_NEG),
        0xd9e1 => Operation::Unary(Op::F64_ABS),
        0xd9fa => Operation::Unary(Op::F64_SQRT),
        0xd9fc => Operation::Round,
        0xd9e8 => Operation::Constant(1.0),
        0xd9e9 => Operation::Constant(std::f64::consts::LOG2_10),
        0xd9ea => Operation::Constant(std::f64::consts::LOG2_E),
        0xd9eb => Operation::Constant(std::f64::consts::PI),
        0xd9ec => Operation::Constant(std::f64::consts::LOG10_2),
        0xd9ed => Operation::Constant(std::f64::consts::LN_2),
        0xd9ee => Operation::Constant(0.0),
        _ => return None,
    })
}

fn math(field: u32) -> Option<(u8, bool)> {
    Some(match field {
        0 => (Op::F64_ADD, false),
        1 => (Op::F64_MUL, false),
        4 => (Op::F64_SUB, false),
        5 => (Op::F64_SUB, true),
        6 => (Op::F64_DIV, false),
        7 => (Op::F64_DIV, true),
        _ => return None,
    })
}

pub(crate) struct Run {
    pub count: usize,
    pub code: Seq,
    // (instruction index, relative Wasm offset), including the fallback path.
    pub offsets: Vec<(usize, u32)>,
}

pub(crate) fn emit(insns: &[&Insn]) -> Option<Run> {
    // Four-operation runs did not recover the cost of their range guards in
    // alternating benchmarks. Six operations is the first measured win.
    emit_minimum(insns, 6)
}

fn emit_minimum(insns: &[&Insn], minimum: usize) -> Option<Run> {
    let operations: Vec<_> = insns.iter().take(32).map_while(|i| decode(i)).collect();
    if operations.len() < minimum {
        return None;
    }
    let mut code = reg(abi::TEB_BASE);
    code.extend(i32c(abi::FP_OFFSET));
    code.push(Op::I32_ADD);
    code.extend(local_set(T1)); // bank address, with the original i32 wrapping
    code.extend(reg(abi::FP_TOP));
    code.extend(local_set(T2)); // original TOP remains available throughout
    code.extend([0x3f, 0]); // memory.size, evaluated once and widened before shift
    code.push(Op::I64_EXTEND_I32_U);
    code.extend(i64c(16));
    code.push(Op::I64_SHL);
    code.extend(local_set(FULL));
    code.extend(wide(T1));
    code.extend(i64c(64));
    code.push(Op::I64_ADD);
    code.extend(local_get(FULL));
    code.push(0x58); // i64.le_u
    let mut ranges = Vec::new();
    for operation in &operations {
        let (m, wide) = match operation {
            Operation::Load(m, wide)
            | Operation::Store(m, wide, _)
            | Operation::MemoryMath(m, wide, _, _) => (*m, *wide),
            _ => continue,
        };
        // Repeated operands need one guard. They still execute in source order.
        if !ranges.contains(&(m, wide)) {
            ranges.push((m, wide));
            code.extend(guard(m, if wide { 8 } else { 4 }));
            code.push(Op::I32_AND);
        }
    }
    code.extend([Op::IF, 0x40]);
    let mut state = State::default();
    let mut offsets = vec![(0, 0)];
    for (i, operation) in operations.iter().enumerate() {
        offsets.push((i, code.len() as u32));
        state.emit(operation, &mut code);
    }
    state.flush(&mut code);
    code.push(Op::ELSE);
    for (i, insn) in insns.iter().take(operations.len()).enumerate() {
        offsets.push((i, code.len() as u32));
        code.extend(emit_fpu(insn).ok()?);
    }
    code.push(Op::END);
    Some(Run {
        count: operations.len(),
        code,
        offsets,
    })
}

fn wide(local: u32) -> Seq {
    let mut code = local_get(local);
    code.push(Op::I64_EXTEND_I32_U);
    code
}

pub(super) fn guard(m: &MemOp, bytes: i64) -> Seq {
    let mut code = emit_address(m);
    code.extend(local_set(T3));
    code.extend(wide(T3));
    code.extend(i64c(bytes));
    code.push(Op::I64_ADD);
    code.extend(local_get(FULL));
    code.push(0x58); // end <= memory length
    code.extend(wide(T3));
    code.extend(i64c(bytes));
    code.push(Op::I64_ADD);
    code.extend(wide(T1));
    code.push(0x58); // end <= bank start
    code.extend(wide(T3));
    code.extend(wide(T1));
    code.extend(i64c(64));
    code.push(Op::I64_ADD);
    code.push(0x5a); // start >= bank end
    code.extend([Op::I32_OR, Op::I32_AND]);
    code
}

#[derive(Default)]
struct State {
    top: i32,
    top_changed: bool,
    loaded: [bool; 8],
    dirty: [bool; 8],
}

impl State {
    fn index(&self, i: i32) -> usize {
        ((self.top + i) & 7) as usize
    }

    fn address(index: usize) -> Seq {
        let mut code = local_get(T2);
        code.extend(i32c(index as i32));
        code.push(Op::I32_ADD);
        code.extend(i32c(7));
        code.push(Op::I32_AND);
        code.extend(i32c(3));
        code.push(Op::I32_SHL);
        code.extend(local_get(T1));
        code.push(Op::I32_ADD);
        code
    }

    fn get(&mut self, i: i32, code: &mut Seq) {
        let index = self.index(i);
        if !self.loaded[index] {
            code.extend(load_f64(Self::address(index)));
            code.extend(local_set(FP_LOCALS + index as u32));
            self.loaded[index] = true;
        }
        code.extend(local_get(FP_LOCALS + index as u32));
    }

    fn set(&mut self, i: i32, code: &mut Seq) {
        let index = self.index(i);
        code.extend(local_set(FP_LOCALS + index as u32));
        self.loaded[index] = true;
        self.dirty[index] = true;
    }

    fn shift(&mut self, delta: i32) {
        self.top = (self.top + delta) & 7;
        self.top_changed = true;
    }

    fn emit(&mut self, operation: &Operation<'_>, code: &mut Seq) {
        match *operation {
            Operation::Load(m, wide) => {
                self.shift(-1);
                code.extend(read_f64(m, if wide { "f64" } else { "f32" }));
                self.set(0, code);
            }
            Operation::Store(m, wide, pop) => {
                code.extend(emit_address(m));
                self.get(0, code);
                if !wide {
                    code.push(Op::F32_DEMOTE_F64);
                }
                code.extend(if wide {
                    [Op::F64_STORE, 3, 0]
                } else {
                    [Op::F32_STORE, 2, 0]
                });
                if pop {
                    self.shift(1);
                }
            }
            Operation::MemoryMath(m, wide, op, reverse) => {
                if reverse {
                    code.extend(read_f64(m, if wide { "f64" } else { "f32" }));
                    self.get(0, code);
                } else {
                    self.get(0, code);
                    code.extend(read_f64(m, if wide { "f64" } else { "f32" }));
                }
                code.push(op);
                self.set(0, code);
            }
            Operation::RegisterMath(destination, left, right, op, pop) => {
                self.get(left, code);
                self.get(right, code);
                code.push(op);
                self.set(destination, code);
                if pop {
                    self.shift(1);
                }
            }
            Operation::Copy(destination, source, pop) => {
                self.get(source, code);
                self.set(destination, code);
                if pop {
                    self.shift(1);
                }
            }
            Operation::Push(i) => {
                self.get(i, code);
                self.shift(-1);
                self.set(0, code);
            }
            Operation::Swap(i) => {
                self.get(i, code);
                self.get(0, code);
                self.set(i, code);
                self.set(0, code);
            }
            Operation::Unary(op) => {
                self.get(0, code);
                code.push(op);
                self.set(0, code);
            }
            Operation::Round => {
                let mut value = Vec::new();
                self.get(0, &mut value);
                code.extend(round_integer(value));
                self.set(0, code);
            }
            Operation::Constant(value) => {
                self.shift(-1);
                code.extend(f64c(value));
                self.set(0, code);
            }
        }
    }

    fn flush(&self, code: &mut Seq) {
        for (index, &dirty) in self.dirty.iter().enumerate() {
            if dirty {
                code.extend(store_f64(
                    Self::address(index),
                    local_get(FP_LOCALS + index as u32),
                ));
            }
        }
        if self.top_changed {
            let mut top = local_get(T2);
            top.extend(i32c(self.top));
            top.push(Op::I32_ADD);
            top.extend(i32c(7));
            top.push(Op::I32_AND);
            code.extend(set_reg(abi::FP_TOP, top));
        }
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/translate/emit_fpu/runs.rs"]
mod tests;
