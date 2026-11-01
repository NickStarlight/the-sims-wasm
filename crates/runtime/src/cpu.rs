//! Coarse CPU helpers shared by native and browser execution hosts.
use crate::{Error, GuestMemory, Process, Result};

const EAX: usize = 0;
const ECX: usize = 1;
const EDX: usize = 2;
const EBX: usize = 3;
const FP_TOP: usize = 9;
const TEB_BASE: usize = 15;
const FPU_OFFSET: u32 = 0x400;
const CYCLES_PER_MILLISECOND: f64 = 3_000_000.0;

impl<M: GuestMemory> Process<M> {
    /// Execute compiler helper slots 1..10 without a Windows call/return transition.
    /// The host supplies all imported register globals in compiler ABI order.
    pub fn cpu_helper(
        &mut self,
        id: u32,
        mut registers: [i32; 16],
        monotonic_ms: f64,
    ) -> Result<[i32; 16]> {
        match id {
            1..=8 => self.fpu_helper(id, &mut registers)?,
            9 => timestamp(&mut registers, monotonic_ms)?,
            10 => cpuid(&mut registers),
            _ => return Err(Error::UnknownCpuHelper(id)),
        }
        Ok(registers)
    }

    fn fpu_helper(&mut self, id: u32, registers: &mut [i32; 16]) -> Result<()> {
        let address = (registers[TEB_BASE] as u32)
            .checked_add(FPU_OFFSET)
            .ok_or(Error::InvalidRange)?;
        let mut bytes = [0; 64];
        self.read(address, &mut bytes)?;
        let mut values = [0.0; 8];
        for (value, bytes) in values.iter_mut().zip(bytes.as_chunks::<8>().0) {
            *value = f64::from_le_bytes(*bytes);
        }
        let top = (registers[FP_TOP] & 7) as usize;
        let next = (top + 1) & 7;
        let pushed = (top + 7) & 7;
        let x = values[top];
        let y = values[next];
        let mut new_top = registers[FP_TOP];
        match id {
            1 | 3 | 5 => {
                values[next] = match id {
                    1 => libm::atan2(y, x),
                    3 => y * libm::log2(x),
                    _ => y * libm::log1p(x) * std::f64::consts::LOG2_E,
                };
                new_top = next as i32;
            }
            2 | 8 => {
                values[top] = if id == 2 { libm::tan(x) } else { libm::sin(x) };
                values[pushed] = if id == 2 { 1.0 } else { libm::cos(x) };
                new_top = pushed as i32;
            }
            4 => values[top] = libm::expm1(x * std::f64::consts::LN_2),
            6 => values[top] = libm::sin(x),
            7 => values[top] = libm::cos(x),
            _ => return Err(Error::UnknownCpuHelper(id)),
        }
        for (bytes, value) in bytes.as_chunks_mut::<8>().0.iter_mut().zip(values) {
            bytes.copy_from_slice(&value.to_le_bytes());
        }
        // Validate/write the complete bank before publishing the changed stack top.
        self.write(address, &bytes)?;
        registers[FP_TOP] = new_top;
        Ok(())
    }
}

fn timestamp(registers: &mut [i32; 16], monotonic_ms: f64) -> Result<()> {
    let ticks = libm::floor(monotonic_ms * CYCLES_PER_MILLISECOND);
    if !ticks.is_finite() || ticks < 0.0 || ticks >= 18_446_744_073_709_551_616.0 {
        return Err(Error::InvalidClock);
    }
    let ticks = ticks as u64;
    registers[EAX] = ticks as i32;
    registers[EDX] = (ticks >> 32) as i32;
    Ok(())
}

fn cpuid(registers: &mut [i32; 16]) {
    match registers[EAX] as u32 {
        0 => {
            registers[EAX] = 1;
            registers[EBX] = i32::from_le_bytes(*b"Genu");
            registers[EDX] = i32::from_le_bytes(*b"ineI");
            registers[ECX] = i32::from_le_bytes(*b"ntel");
        }
        1 => {
            registers[EAX] = 0x633;
            registers[EBX] = 0;
            registers[ECX] = 0;
            // Only x87 and RDTSC: no MMX, SSE, CMOV, or CMPXCHG8B lowering.
            registers[EDX] = (1 << 0) | (1 << 4);
        }
        _ => registers[..4].fill(0),
    }
}

#[cfg(test)]
#[path = "../tests/unit/cpu.rs"]
mod tests;
