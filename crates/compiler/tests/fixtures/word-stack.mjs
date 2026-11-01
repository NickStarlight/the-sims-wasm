import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';

const names = ['eax', 'ecx', 'edx', 'ebx', 'esp', 'ebp', 'esi', 'edi',
  'flags', 'fp_top', 'fp_status', 'trap_va', 'call_target', 'resume_next',
  'actual_return', 'teb_base'];
const env = Object.fromEntries(names.map(name => [name,
  new WebAssembly.Global({ value: 'i32', mutable: true }, 0)]));
env.memory = new WebAssembly.Memory({ initial: 1 });
const { exports: run } = new WebAssembly.Instance(new WebAssembly.Module(readFileSync(0)), { env });
const bytes = new Uint8Array(env.memory.buffer);
const memory = new DataView(env.memory.buffer);
const originalFlags = 0x240ad7;
const initialSp = 0x1000;
function reset() {
  bytes.fill(0x5a);
  for (const name of names) env[name].value = 0;
  env.esp.value = initialSp;
  env.eax.value = 0xaabb1234;
  env.flags.value = originalFlags;
}
function pushed(name, width, value) {
  reset();
  memory.setUint32(initialSp, 0xcdef9876, true);
  run[name]();
  assert.equal(env.esp.value, initialSp - width, `${name}: stack width`);
  assert.equal(width === 2 ? memory.getUint16(initialSp - width, true)
    : memory.getUint32(initialSp - width, true), value, `${name}: value`);
  assert.equal(memory.getUint16(initialSp - width - 2, true), 0x5a5a, `${name}: lower canary`);
  assert.equal(memory.getUint32(initialSp, true), 0xcdef9876, `${name}: upper canary`);
  assert.equal(env.flags.value, originalFlags, `${name}: flags`);
}
pushed('push_ax', 2, 0x1234);
pushed('push_sp', 2, initialSp);
pushed('push_esp', 4, initialSp);
pushed('push_mem16', 2, 0x9876);
pushed('push_mem32', 4, 0xcdef9876);
pushed('push_imm8_16', 2, 0xffff);
pushed('push_imm16', 2, 0x1234);
pushed('push_flags16', 2, originalFlags & 0xffff);
pushed('push_flags32', 4, originalFlags);

reset();
memory.setUint16(initialSp, 0x9876, true);
run.pop_ax();
assert.equal(env.eax.value >>> 0, 0xaabb9876);
assert.equal(env.esp.value, initialSp + 2);
assert.equal(env.flags.value, originalFlags);
for (const width of [2, 4]) {
  reset();
  memory.setUint32(initialSp, 0xabcd9876, true);
  run[`pop_mem${width * 8}`]();
  assert.equal(env.esp.value, initialSp + width);
  assert.equal(width === 2 ? memory.getUint16(initialSp + width, true)
    : memory.getUint32(initialSp + width, true), width === 2 ? 0x9876 : 0xabcd9876);
  assert.equal(memory.getUint16(initialSp + width * 2, true), 0x5a5a);
  assert.equal(env.flags.value, originalFlags);
}
for (const name of ['pop_sp', 'pop_esp']) {
  reset();
  memory.setUint32(initialSp, 0x2345, true);
  run[name]();
  assert.equal(env.esp.value, 0x2345, `${name}: popped pointer must replace incremented ESP`);
}
for (const width of [2, 4]) {
  reset();
  memory.setUint32(initialSp, 0x200041, true);
  run[`pop_flags${width * 8}`]();
  assert.equal(env.flags.value, width === 2 ? (originalFlags & 0xffff0000) | 0x41 : 0x200041);
  assert.equal(env.esp.value, initialSp + width);
}
reset();
memory.setUint32(initialSp, 0x12345678, true);
run.cpu_flags_roundtrip();
assert.equal(env.esp.value, initialSp + 4);
assert.equal(env.actual_return.value, 0x12345678);
assert.equal(env.eax.value >>> 0, (0xaabb0000 | (originalFlags & 0xffff)) >>> 0);
assert.equal(env.flags.value, originalFlags);
console.log('word stack, flags, ESP-relative memory, and RET cases passed');

// Operand-size override on MOVSX/MOVZX writes AX, preserving high EAX bits.
for (const byte of [0, 1, 0x7f, 0x80, 0xff]) {
  for (const operation of ['movsx', 'movzx']) {
    reset();
    env.ecx.value = 0x12340000 | byte;
    run[`${operation}_ax_cl`]();
    const word = operation === 'movsx' && byte >= 0x80 ? 0xff00 | byte : byte;
    assert.equal(env.eax.value >>> 0, (0xaabb0000 | word) >>> 0);
    assert.equal(env.flags.value, originalFlags);
    assert.equal(env.esp.value, initialSp);
  }
}
