import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
const names = [
  'eax',
  'ecx',
  'edx',
  'ebx',
  'esp',
  'ebp',
  'esi',
  'edi',
  'flags',
  'fp_top',
  'fp_status',
  'trap_va',
  'call_target',
  'resume_next',
  'actual_return',
  'teb_base',
]
const env = {
  memory: new WebAssembly.Memory({ initial: 2 }),
  observe(address) {
    traces.push({
      address,
      registers: names.map((n) => env[n].value),
      stack: bytes.slice(0x1fe0, 0x2020),
    })
    if (change && traces.length === 1) {
      bytes[change] = 0xff
      env.ecx.value = 0x80
      env.esi.value = 0xabcdef01
    }
    if (fail && traces.length === fail) throw new Error('checkpoint stop')
  },
}
for (const name of names) env[name] = new WebAssembly.Global({ value: 'i32', mutable: true }, 0)
const module = new WebAssembly.Module(readFileSync(0))
const { exports: run } = new WebAssembly.Instance(module, { env })
const bytes = new Uint8Array(env.memory.buffer),
  view = new DataView(bytes.buffer)
let traces = [],
  change = 0,
  fail = 0,
  cases = 0,
  seed = 19
const random = () => (seed = (Math.imul(seed, 1664525) + 1013904223) | 0)
const hash = (data) => {
  let h = 0
  for (const byte of data) {
    if (!byte) break
    h = Math.imul(h + ((byte << 24) >> 24), 0x401)
    h ^= h >>> 6
  }
  h = Math.imul(h, 9)
  return Math.imul(h ^ (h >>> 11), 0x8001) >>> 0
}
function compare(
  data,
  {
    pointer = 0x4000,
    stack = 0x2000,
    budget = 256,
    polling = true,
    mutation = 0,
    stop = 0,
    unterminated = false,
    resume = false,
  } = {},
) {
  const state = names.map(() => random())
  function execute(name) {
    bytes.fill(0xa5)
    if (pointer + data.length <= bytes.length) bytes.set(data, pointer)
    if (!unterminated && pointer + data.length < bytes.length) bytes[pointer + data.length] = 0
    if (stack + 8 <= bytes.length) {
      view.setUint32(stack, 0, true)
      view.setUint32(stack + 4, pointer, true)
    }
    names.forEach((n, i) => (env[n].value = state[i]))
    env.eax.value = budget
    run.set_budget()
    names.forEach((n, i) => (env[n].value = state[i]))
    env.esp.value = stack
    env.resume_next.value = 0
    if (resume) {
      view.setUint32(stack, state[6], true)
      view.setUint32(stack + 4, 0, true)
      view.setUint32(stack + 8, pointer, true)
      env.edx.value = pointer
      env.ecx.value = data[0] ?? 0
    }
    traces = []
    change = mutation
    fail = stop
    let error
    try {
      run[name + (resume ? '_resume' : polling ? '' : '_unpolled')]()
    } catch (e) {
      error = {
        name: e.name,
        message: e instanceof WebAssembly.RuntimeError ? 'wasm trap' : e.message,
      }
    }
    const registers = names.map((n) => env[n].value)
    run.read_budget()
    return { registers, budget: env.eax.value, memory: bytes.slice(), traces, error }
  }
  const before = execute('plain'),
    after = execute('fast')
  assert.deepEqual(
    after,
    before,
    JSON.stringify({
      length: data.length,
      pointer,
      stack,
      budget,
      polling,
      mutation,
      stop,
      unterminated,
      resume,
    }),
  )
  if (
    !before.error &&
    !mutation &&
    pointer >= 0x4000 &&
    pointer + data.length < bytes.length &&
    !unterminated &&
    !resume
  )
    assert.equal(before.registers[0] >>> 0, hash(data), 'independent signed-byte hash')
  cases++
}
for (const polling of [false, true])
  for (const length of [0, 1, 2, 7, 31, 63, 64, 65, 127, 128, 255, 256, 511, 1024]) {
    for (const budget of [0, 1, 2, 3, 7, 63, 64, 65, 127, 255, 256, -1]) {
      const data = Uint8Array.from({ length }, () => 1 + ((random() >>> 0) % 255))
      compare(data, { polling, budget })
    }
  }
for (let byte = 1; byte < 256; byte++) compare(Uint8Array.of(byte), { budget: 2 })
for (const length of [0, 1, 2, 63, 64, 65, 256]) {
  const data = Uint8Array.from({ length }, (_, i) => 1 + ((i * 73) % 255))
  compare(data, { pointer: bytes.length - length - 1 })
  compare(data, { pointer: bytes.length - length, unterminated: true })
}
for (const stack of [0, 1, 2, bytes.length - 8, bytes.length - 7, bytes.length, 0xffffffff])
  compare(Uint8Array.from([65, 0x80, 0xff]), { stack })
for (const budget of [2, 3, 7, 63, 64, 65, 255, 256]) {
  const data = Uint8Array.from({ length: 1024 }, (_, i) => 1 + (i % 255))
  compare(data, { budget, mutation: 0x4040 })
  compare(data, { budget, stop: 1 })
  compare(data, { budget, stop: 2 })
}
// Stack/source overlap, including the original PUSH ESI changing future input.
for (const pointer of [0x1ff9, 0x1ffc, 0x2000, 0x2004, 0x2008])
  compare(
    Uint8Array.from({ length: 80 }, () => 65),
    { pointer },
  )
for (const length of [1, 2, 3, 63, 64, 65, 127, 256])
  for (const budget of [1, 2, 3, 7, 63, 64, 65, 255])
    compare(
      Uint8Array.from({ length }, (_, i) => 1 + ((i * 73) % 255)),
      { resume: true, budget },
    )

// A real checkpoint grows the memory and supplies the remaining source bytes.
function growth(name) {
  const observed = []
  let grew = false
  const host = {
    memory: new WebAssembly.Memory({ initial: 1, maximum: 3 }),
    observe(address) {
      observed.push({
        address,
        registers: names.map((n) => host[n].value),
        size: host.memory.buffer.byteLength,
      })
      if (!grew) {
        grew = true
        host.memory.grow(1)
        const b = new Uint8Array(host.memory.buffer)
        b.fill(66, 65536, 65568)
        b[65568] = 0
      }
    },
  }
  for (const [i, n] of names.entries())
    host[n] = new WebAssembly.Global({ value: 'i32', mutable: true }, i * 997)
  const { exports: r } = new WebAssembly.Instance(module, { env: host })
  const initial = new Uint8Array(host.memory.buffer),
    v = new DataView(initial.buffer)
  initial.fill(65, 65504)
  v.setUint32(0x2000, 0, true)
  v.setUint32(0x2004, 65504, true)
  host.eax.value = 8
  r.set_budget()
  host.eax.value = 19
  host.esp.value = 0x2000
  host.resume_next.value = 0
  r[name]()
  assert.ok(grew)
  const registers = names.map((n) => host[n].value)
  r.read_budget()
  return {
    registers,
    budget: host.eax.value,
    memory: new Uint8Array(host.memory.buffer).slice(),
    observed,
  }
}
assert.deepEqual(growth('fast'), growth('plain'))
cases++
console.log(`Hash spans: ${cases} native-state/checkpoint/fault/byte cases passed`)
