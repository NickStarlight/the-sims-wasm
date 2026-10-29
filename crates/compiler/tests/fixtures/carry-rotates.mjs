// Independent width+carry ring model; input is a Rust-emitted Wasm test module.
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';

const names = ['eax', 'ecx', 'edx', 'ebx', 'esp', 'ebp', 'esi', 'edi',
  'flags', 'fp_top', 'fp_status', 'trap_va', 'call_target', 'resume_next',
  'actual_return', 'teb_base'];
const env = Object.fromEntries(names.map(name => [name,
  new WebAssembly.Global({ value: 'i32', mutable: true }, 0)]));
env.memory = new WebAssembly.Memory({ initial: 1 });
const instance = new WebAssembly.Instance(new WebAssembly.Module(readFileSync(0)), { env });
const mask32 = 0xffffffffn;

function reference(direction, bits, eax, count, flags) {
  const width = BigInt(bits);
  const mask = (1n << width) - 1n;
  const ringWidth = width + 1n;
  const ringMask = (1n << ringWidth) - 1n;
  const original = BigInt(eax) & mask;
  const maskedCount = count & 31;
  const steps = BigInt(maskedCount % (bits + 1));
  const ring = original | (BigInt(flags & 1) << width);
  const rotated = steps === 0n ? ring : direction === 'rcl'
    ? ((ring << steps) | (ring >> (ringWidth - steps))) & ringMask
    : ((ring >> steps) | (ring << (ringWidth - steps))) & ringMask;
  const result = rotated & mask;
  let expectedFlags = flags;
  if (maskedCount !== 0) {
    const carry = Number((rotated >> width) & 1n);
    expectedFlags = (flags & ~1) | carry;
    if (maskedCount === 1) {
      const high = Number(result >> (width - 1n));
      const overflow = direction === 'rcl' ? high ^ carry
        : high ^ Number((result >> (width - 2n)) & 1n);
      expectedFlags = (expectedFlags & ~0x800) | (overflow << 11);
    }
  }
  return { eax: Number(((BigInt(eax) & (mask32 ^ mask)) | result) & mask32),
    flags: expectedFlags >>> 0 };
}

let cases = 0;
for (const direction of ['rcl', 'rcr']) {
  for (const bits of [8, 16, 32]) {
    for (const eax of [0, 1, 0x7fffffff, 0x80000000, 0xffffffff, 0xa5c37e91, 0x12348080]) {
      for (const count of [0, 1, 2, 7, 8, 9, 10, 15, 16, 17, 18, 31, 32, 33, 63, 255]) {
        for (const flags of [0, 1, 0x8d4, 0x8d5, 0xcd4, 0xcd5]) {
          const expected = reference(direction, bits, eax, count, flags);
          const label = `${direction}${bits} eax=${eax.toString(16)} count=${count} flags=${flags.toString(16)}`;
          env.eax.value = eax;
          env.ecx.value = count;
          env.flags.value = flags;
          env.esp.value = 0x1000;
          env.actual_return.value = -1;
          new DataView(env.memory.buffer).setUint32(0x1000, 0x12345678, true);
          instance.exports[`${direction}${bits}`]();
          assert.equal(env.eax.value >>> 0, expected.eax, `${label}: result`);
          // OF is undefined for multi-bit rotations; this emitter preserves it.
          const maskedCount = count & 31;
          const effectiveCount = maskedCount % (bits + 1);
          const definedMask = maskedCount > 1 && effectiveCount !== 0 ? ~0x800 : -1;
          assert.equal((env.flags.value & definedMask) >>> 0,
            (expected.flags & definedMask) >>> 0, `${label}: flags`);
          assert.equal(env.ecx.value >>> 0, count, `${label}: ECX preservation`);
          assert.equal(env.esp.value, 0x1004, `${label}: RET stack`);
          assert.equal(env.actual_return.value, 0x12345678, `${label}: RET address`);
          cases++;
        }
      }
    }
  }
}
console.log(`${cases} carry rotation cases passed`);
