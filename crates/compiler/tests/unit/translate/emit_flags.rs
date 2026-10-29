use crate::translate::test_support::{execute_optimized_v8, execute_v8};

const ASSERTIONS: &str = r#"
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
const names = ['eax','ecx','edx','ebx','esp','ebp','esi','edi','flags','fp_top','fp_status','trap_va','call_target','resume_next','actual_return','teb_base'];
const env = { memory: new WebAssembly.Memory({ initial: 1 }) };
for (const name of names) env[name] = new WebAssembly.Global({ value: 'i32', mutable: true }, 0);
const { exports: run } = new WebAssembly.Instance(new WebAssembly.Module(readFileSync(0)), { env });
for (let value = 0; value < 65536; value++) {
  env.eax.value = value | 0x5a5a0000;
  env.ecx.value = (value * 73) | 0;
  env.esp.value = 0x1000;
  run.parity();
  let bits = value & 255, count = 0;
  while (bits) { count += bits & 1; bits >>>= 1; }
  assert.equal(env.flags.value & 4, count % 2 ? 0 : 4);
  env.eax.value = value; env.esp.value = 0x1000;
  run.overwritten();
  assert.equal(env.eax.value, ((value + 17) ^ 0x55) | 0);
  assert.equal(env.flags.value & 0x40, env.eax.value === 0 ? 0x40 : 0);
}
// A fault in the second instruction must retain flags from the first.
env.eax.value = -1; env.edx.value = 0x10000; env.esp.value = 0x1000;
assert.throws(() => run.fault(), WebAssembly.RuntimeError);
assert.equal(env.flags.value & 0x41, 0x41);
// Carry consumers and partial-register flag preservation cannot use this peephole.
env.eax.value = -1; env.esp.value = 0x1000; run.carry();
assert.equal(env.eax.value, 1);
"#;

fn routines() -> Vec<(&'static str, Vec<u8>)> {
    vec![
        ("parity", vec![0x83, 0xc0, 0, 0xc3]),
        ("overwritten", vec![0x83, 0xc0, 17, 0x83, 0xf0, 0x55, 0xc3]),
        ("fault", vec![0x83, 0xc0, 1, 0x03, 0x02, 0xc3]),
        ("carry", vec![0x83, 0xc0, 1, 0x83, 0xd0, 0, 0xc3]),
    ]
}

#[test]
fn flags_match_exhaustive_parity_and_preserve_carry_and_fault_state() {
    execute_v8(&routines(), ASSERTIONS);
    execute_optimized_v8(&routines(), ASSERTIONS);
}
