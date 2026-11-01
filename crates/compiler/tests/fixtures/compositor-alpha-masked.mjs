import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { createHash } from 'node:crypto'

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
const memory = new WebAssembly.Memory({ initial: 128 })
const bytes = new Uint8Array(memory.buffer)
const view = new DataView(memory.buffer)
const alpha = 0x6e5c0c,
  previous = 0x6e5d0c,
  table = 0x6e5e5c
let traces, mutation
const env = {
  memory,
  observe() {
    traces.push(snapshot())
    mutation?.(traces.length)
  },
}
for (const name of names) env[name] = new WebAssembly.Global({ value: 'i32', mutable: true }, 0)
const { exports } = new WebAssembly.Instance(new WebAssembly.Module(readFileSync(0)), { env })
function snapshot() {
  return {
    registers: names.map((name) => env[name].value),
    memory: createHash('sha256').update(bytes).digest('hex'),
  }
}
let seed = 19,
  cases = 0
const random = () => (seed = (Math.imul(seed, 1664525) + 1013904223) | 0)
const valid = (at, count) => at >= 0 && at + count <= bytes.length
const put = (at, value) => {
  if (valid(at, 4)) view.setUint32(at, value, true)
}
function compare(options = {}) {
  const {
    index = 0,
    budget = 256,
    frame = 0x18000,
    stack = 0x17fc0,
    destination = 0x20000,
    source = 0x30000,
    mask = 31,
    blendMask = 0xaaaaaaaa,
    bit,
    badAlpha,
    change,
    interrupt,
  } = options
  const initial = names.map(() => random())
  function run(name) {
    bytes.fill(0xa5)
    for (let i = 0; i < 32768; i++) bytes[table + i] = (i * 73 + (i >>> 7) * 19) & 255
    for (let i = 0; i < 32; i++) {
      view.setUint16(alpha + 2 * i, (i * 19 + 7) & 255, true)
      if (valid(source + 2 * i, 2)) view.setUint16(source + 2 * i, (i * 9173) & 65535, true)
      view.setUint16(previous + 2 * i, (i * 3147 + 99) & 65535, true)
    }
    if (badAlpha !== undefined) view.setUint16(alpha + 62, badAlpha, true)
    put(frame - 8, source)
    put(frame + 28, blendMask)
    put(frame - 12, bit ?? 1 << index)
    put(frame + 8, (destination - source) >>> 0)
    env.eax.value = budget
    exports.set_budget()
    names.forEach((name, i) => (env[name].value = initial[i]))
    env.esi.value = index * 2
    env.ebp.value = frame
    env.esp.value = stack
    env.edi.value = mask
    traces = []
    mutation = (count) => {
      if (count !== 1) return
      if (change === 'mask') put(frame + 28, 0x33333333)
      if (change === 'alpha') view.setUint16(alpha + 62, 0xffff, true)
      if (change === 'destination') put(frame + 8, (alpha - source) >>> 0)
      if (interrupt) throw new Error('checkpoint interrupt')
    }
    let error = null
    try {
      exports[name]()
    } catch (e) {
      error = { name: e.name, message: e.message }
    }
    const state = snapshot()
    exports.read_budget()
    return { ...state, budget: env.eax.value, error, traces }
  }
  assert.deepEqual(run('span'), run('plain'), JSON.stringify(options))
  cases++
}
for (const index of [0, 1, 7, 15, 30, 31])
  for (const budget of [1, 2, 3, 7, 16, 31, 32, 33, 63, 64, 65, 95, 96, 97, 256])
    for (const blendMask of [0, 1, 0x80000000, 0xffffffff, 0xaaaaaaaa, 0x55555555])
      compare({ index, budget, blendMask })
for (const destination of [
  bytes.length - 64,
  bytes.length - 63,
  bytes.length - 2,
  bytes.length,
  alpha,
  previous,
  table,
  0x17ff4,
  0x1801c,
  0x30000,
  0x30002,
])
  compare({ destination })
for (const source of [
  bytes.length - 64,
  bytes.length - 63,
  bytes.length - 2,
  bytes.length,
  alpha,
  previous,
  table,
  0x17ff4,
  0x1801c,
  0x20000,
  0x20002,
])
  compare({ source })
for (const frame of [0, 8, bytes.length - 28, bytes.length, alpha + 12, previous + 12, table + 12])
  compare({ frame })
for (const badAlpha of [256, 0x7fff, 0x8000, 0xffff]) compare({ badAlpha })
for (const mask of [0, 7, 0xffff, -1]) compare({ mask })
for (const index of [32, -1, 0x10000000]) compare({ index })
for (const bit of [0, 2, 0x80000000, -1]) compare({ bit })
for (const change of ['alpha', 'destination', 'mask']) compare({ budget: 4, change })
compare({ budget: 4, interrupt: true })
console.log(
  `Masked alpha compositor spans: ${cases} checkpoint/register/memory/fault/alias cases passed`,
)
