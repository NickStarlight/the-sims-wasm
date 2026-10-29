#[test]
fn bit_scan_counts_and_zero_flag_execute_in_wasm() {
    crate::translate::test_support::execute_v8(
        &[
            ("forward", vec![0x0f, 0xbc, 0xc8]),    // BSF ECX,EAX
            ("reverse", vec![0x0f, 0xbd, 0xc8]),    // BSR ECX,EAX
            ("word", vec![0x66, 0x0f, 0xbc, 0xc8]), // BSF CX,AX
        ],
        r#"
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
const env = { memory: new WebAssembly.Memory({ initial: 1 }) };
for (const name of ['eax','ecx','edx','ebx','esp','ebp','esi','edi','flags','fp_top','fp_status','trap_va','call_target','resume_next','actual_return','teb_base'])
  env[name] = new WebAssembly.Global({ value: 'i32', mutable: true }, 0);
const { exports } = new WebAssembly.Instance(new WebAssembly.Module(readFileSync(0)), { env });
for (let bit = 0; bit < 32; bit++) {
  env.eax.value = 1 << bit;
  for (const name of ['forward', 'reverse']) {
    env.ecx.value = 99;
    exports[name]();
    assert.equal(env.ecx.value, bit);
    assert.equal(env.flags.value & 0x40, 0);
  }
}
env.eax.value = 0x80001000;
exports.forward(); assert.equal(env.ecx.value, 12);
exports.reverse(); assert.equal(env.ecx.value, 31);
env.ecx.value = 0x1234ffff;
exports.word(); assert.equal(env.ecx.value, 0x1234000c);
for (const name of ['forward', 'reverse', 'word']) {
  env.eax.value = 0; env.ecx.value = 123;
  exports[name]();
  assert.equal(env.ecx.value, 123);
  assert.equal(env.flags.value & 0x40, 0x40);
}
"#,
    );
}
