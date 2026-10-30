//! Promote repeated CPU-global accesses within straight-line Wasm.
//! Publish dirty values before every possible trap, but retain clean locals
//! across memory operations: guest memory cannot modify CPU-register globals.
use super::{
    emit_helpers::{local_get, local_set, reg, set_reg},
    layout::CACHED_REGISTERS,
};
use wasmparser::{BinaryReader, Operator, OperatorsReader};

const REGISTERS: usize = 11;

#[derive(Clone, Copy)]
enum Access {
    Read(usize),
    Write(usize),
    Memory,
    Pure,
    Barrier,
}
struct Instruction {
    start: usize,
    end: usize,
    access: Access,
}

pub(super) fn promote(code: &[u8]) -> (Vec<u8>, Vec<(u32, u32)>) {
    let mut reader = OperatorsReader::new(BinaryReader::new(code, 0));
    let mut instructions = Vec::new();
    while !reader.eof() {
        let start = reader.original_position();
        let operator = reader
            .read()
            .expect("compiler emitted a malformed operator");
        let access = match operator {
            Operator::GlobalGet { global_index } if global_index < REGISTERS as u32 => {
                Access::Read(global_index as usize)
            }
            Operator::GlobalSet { global_index } if global_index < REGISTERS as u32 => {
                Access::Write(global_index as usize)
            }
            _ => match code[start] {
                0x28..=0x3e => Access::Memory,
                // All other instructions are barriers, including integer
                // division, float-to-integer truncation and control.
                0x01
                | 0x1a..=0x24
                | 0x41..=0x6c
                | 0x71..=0x7e
                | 0x83..=0xa7
                | 0xac..=0xad
                | 0xb2..=0xbf => Access::Pure,
                _ => Access::Barrier,
            },
        };
        instructions.push(Instruction {
            start,
            end: reader.original_position(),
            access,
        });
    }
    let mut output = Vec::with_capacity(code.len());
    let mut offsets = Vec::with_capacity(instructions.len() + 1);
    let mut begin = 0;
    while begin < instructions.len() {
        let mut end = begin;
        while end < instructions.len() && !matches!(instructions[end].access, Access::Barrier) {
            end += 1;
        }
        if begin != end {
            let run = &instructions[begin..end];
            let mut uses = [0; REGISTERS];
            let mut written = [false; REGISTERS];
            let mut needs_load = [false; REGISTERS];
            let mut pending = [false; REGISTERS];
            let mut publications = [0; REGISTERS];
            for insn in run {
                match insn.access {
                    Access::Read(r) => {
                        uses[r] += 1;
                        needs_load[r] |= !written[r];
                    }
                    Access::Write(r) => {
                        uses[r] += 1;
                        written[r] = true;
                        pending[r] = true;
                    }
                    Access::Memory => {
                        for r in 0..REGISTERS {
                            publications[r] += usize::from(pending[r]);
                            pending[r] = false;
                        }
                    }
                    _ => {}
                }
            }
            // A write-only register published at every access saves nothing.
            // Cache only when hoisting actually removes a global operation.
            let cached: [bool; REGISTERS] = std::array::from_fn(|r| {
                let published = publications[r] + usize::from(pending[r]);
                uses[r] >= 3 && uses[r] > published + usize::from(needs_load[r])
            });
            let mut dirty = [false; REGISTERS];
            offsets.push((run[0].start as u32, output.len() as u32));
            for r in 0..REGISTERS {
                if cached[r] && needs_load[r] {
                    output.extend(reg(r as u8));
                    output.extend(local_set(CACHED_REGISTERS + r as u32));
                }
            }
            for (position, insn) in run.iter().enumerate() {
                if position != 0 {
                    offsets.push((insn.start as u32, output.len() as u32));
                }
                match insn.access {
                    Access::Read(r) if cached[r] => {
                        output.extend(local_get(CACHED_REGISTERS + r as u32))
                    }
                    Access::Write(r) if cached[r] => {
                        output.extend(local_set(CACHED_REGISTERS + r as u32));
                        dirty[r] = true;
                    }
                    Access::Memory => {
                        publish(&mut output, &mut dirty);
                        output.extend_from_slice(&code[insn.start..insn.end]);
                    }
                    _ => output.extend_from_slice(&code[insn.start..insn.end]),
                }
            }
            publish(&mut output, &mut dirty);
        }
        if end < instructions.len() {
            let insn = &instructions[end];
            offsets.push((insn.start as u32, output.len() as u32));
            output.extend_from_slice(&code[insn.start..insn.end]);
            end += 1;
        }
        begin = end;
    }
    offsets.push((code.len() as u32, output.len() as u32));
    (output, offsets)
}

fn publish(output: &mut Vec<u8>, dirty: &mut [bool; REGISTERS]) {
    for (r, changed) in dirty.iter_mut().enumerate() {
        if *changed {
            output.extend(set_reg(r as u8, local_get(CACHED_REGISTERS + r as u32)));
            *changed = false;
        }
    }
}

#[cfg(test)]
#[path = "../../tests/unit/translate/optimize.rs"]
mod tests;
