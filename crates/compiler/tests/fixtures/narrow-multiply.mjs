import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
const { wasm, registers, roots } = JSON.parse(readFileSync(0, 'utf8'))
const env = { memory: new WebAssembly.Memory({ initial: 256, maximum: 65536 }),
  table: new WebAssembly.Table({ element: 'anyfunc', initial: 300000 }),
  dispatch() { assert.fail('unexpected import') }, blockTrace() {} }
for (const name of registers) env[name] = new WebAssembly.Global({ value: 'i32', mutable: true }, 0)
const { exports } = new WebAssembly.Instance(new WebAssembly.Module(Uint8Array.from(wasm)), { env })
const words = new Uint32Array(env.memory.buffer)
words.set(roots, 0xa00000 / 4)
const signed = (v, bits) => (v << (32 - bits)) >> (32 - bits)
function check(index, a, b, bits, isSigned) {
  const mask = bits === 8 ? 255 : 65535
  const eax = (0x89ab0000 | a | (index === 4 ? b << 8 : 0)) >>> 0
  const edx = (0x76540000 | (index === 5 ? b : 0x3210)) >>> 0
  env.eax.value = eax; env.edx.value = edx; env.ebx.value = 0xcdef0000 | b
  env.esi.value = 0x20000; words[0x20000 / 4] = b
  env.esp.value = 0x10000; words[0x10000 / 4] = 0x12345678
  env.flags.value = 0x8d5; env.resume_next.value = 0
  exports.runAt(roots[index])
  const product = isSigned ? signed(a, bits) * signed(b, bits) : a * b
  const overflow = isSigned ? product !== signed(product, bits) : product > mask
  assert.equal(env.eax.value >>> 0, ((eax & 0xffff0000) | (product & 65535)) >>> 0)
  assert.equal(env.edx.value >>> 0, bits === 8 ? edx : ((edx & 0xffff0000) | ((product >>> 16) & 65535)) >>> 0)
  assert.equal(env.flags.value & 0x801, overflow ? 0x801 : 0)
  assert.equal(env.ebx.value >>> 0, (0xcdef0000 | b) >>> 0)
  assert.equal(env.esp.value, 0x10004)
}
for (let a = 0; a < 256; a++) for (let b = 0; b < 256; b++) {
  check(0, a, b, 8, false); check(1, a, b, 8, true); check(4, a, b, 8, true)
}
const edges = [0, 1, 2, 127, 128, 255, 256, 32767, 32768, 65534, 65535]
for (const a of edges) for (const b of edges) {
  check(2, a, b, 16, false); check(3, a, b, 16, true)
  check(5, a, b, 16, false); check(7, a, b, 16, false)
  check(6, a & 255, b & 255, 8, true)
}
