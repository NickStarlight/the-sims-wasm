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
let traces,
  fail,
  mutate,
  cases = 0,
  seed = 19
const random = () => (seed = (Math.imul(seed, 1664525) + 1013904223) | 0)
const env = {
  memory: new WebAssembly.Memory({ initial: 128 }),
  observe(address) {
    traces.push({
      address,
      registers: names.map((name) => env[name].value),
      point: bytes.slice(0x4000, 0x4004),
    })
    if (mutate && traces.length === 1) {
      bytes[0x4000] = 255
      view.setInt32(0x2004, 3, true)
    }
    if (fail === traces.length) throw new Error('checkpoint stop')
  },
}
for (const name of names) env[name] = new WebAssembly.Global({ value: 'i32', mutable: true }, 0)
const { exports: run } = new WebAssembly.Instance(new WebAssembly.Module(readFileSync(0)), { env })
let bytes = new Uint8Array(env.memory.buffer),
  view = new DataView(bytes.buffer)

function compare({
  pointer = 0x4000,
  stack = 0x2000,
  orientation = 1,
  x = 17,
  y = 23,
  budget = 256,
  stop = 0,
  mutation = false,
  resume = 0,
} = {}) {
  const registers = names.map(() => random())
  const initialize = () => {
    bytes.fill(0xa5)
    for (let i = 0; i < 32768; i++) bytes[0x6de81c + i] = (i * 73 + (i >>> 7)) & 255
    if (pointer < bytes.length) bytes[pointer] = x
    if (pointer + 1 < bytes.length) bytes[pointer + 1] = y
    const word = (at, value) => {
      if (at >= 0 && at + 4 <= bytes.length) view.setUint32(at, value >>> 0, true)
    }
    word(stack, 0)
    word(stack + 4, orientation)
    env.eax.value = budget
    run.set_budget()
    names.forEach((name, i) => {
      env[name].value = registers[i]
    })
    env.esp.value = stack
    env.ecx.value = resume === 0 ? pointer : orientation
    if (resume) env.eax.value = pointer
    env.resume_next.value = resume
    traces = []
    fail = stop
    mutate = mutation
  }
  const execute = (suffix) => {
    initialize()
    let error
    try {
      run['transform_' + suffix]()
    } catch (e) {
      error = {
        name: e.name,
        message: e instanceof WebAssembly.RuntimeError ? 'wasm trap' : e.message,
      }
    }
    const registers = names.map((name) => env[name].value)
    run.read_budget()
    return { registers, budget: env.eax.value, memory: Buffer.from(bytes), traces, error }
  }
  const plain = execute('plain'),
    fast = execute('fast')
  assert.deepEqual(
    fast,
    plain,
    JSON.stringify({
      pointer,
      stack,
      orientation,
      x,
      y,
      budget,
      stop,
      mutation,
      resume,
    }),
  )
  cases++
}

for (const orientation of [0, 1, 2, 3, -1, 0x7fffffff, -2147483648]) {
  for (const [x, y] of [
    [0, 0],
    [63, 63],
    [127, 128],
    [255, 255],
  ])
    compare({ orientation, x, y })
}
for (let i = 0; i < 80; i++)
  compare({ orientation: random(), x: random() & 255, y: random() & 255 })
for (const pointer of [
  bytes.length - 2,
  bytes.length - 1,
  bytes.length,
  0xffffffff,
  0x6de81c,
  0x6e089c,
  0x6e089d,
  // The default (17,23), orientation-one lookup starts at 0x6df0ca.
  // In particular, pointer=lookup+1 overwrites the second lookup byte before
  // the native routine reads it.
  0x6df0c9,
  0x6df0ca,
  0x6df0cb,
  0x6df0cc,
  0x2000,
  0x2004,
])
  compare({ pointer })
for (const stack of [
  bytes.length - 8,
  bytes.length - 4,
  bytes.length - 3,
  bytes.length,
  0xfffffffc,
])
  compare({ stack })
for (const budget of [1, 2, 3, 255, 256])
  for (const resume of [0, 1, 2]) {
    compare({ budget, resume })
    compare({ budget, resume, mutation: true })
    for (const stop of [1, 2, 3]) compare({ budget, resume, stop })
  }
env.memory.grow(1)
bytes = new Uint8Array(env.memory.buffer)
view = new DataView(bytes.buffer)
compare({ pointer: 0x800010 })
console.log(
  `geometry: ${cases} differential cases; registers, full memory, faults and ordered checkpoint snapshots match`,
)
