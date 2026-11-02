import assert from 'node:assert/strict'
import { readFileSync, writeFileSync } from 'node:fs'
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
const bytes = new Uint8Array(memory.buffer),
  view = new DataView(memory.buffer)
const alpha = 0x6e5c0c
let traces, mutation
const snapshot = () => ({
  registers: names.map((n) => env[n].value),
  memory: createHash('sha256').update(bytes).digest('hex'),
})
const env = {
  memory,
  observe() {
    traces.push(snapshot())
    mutation?.(traces.length)
  },
}
for (const name of names) env[name] = new WebAssembly.Global({ value: 'i32', mutable: true }, 0)
const module = new WebAssembly.Module(readFileSync(0))
const { exports: run } = new WebAssembly.Instance(module, { env })
let seed = 137,
  cases = 0
const random = () => (seed = (Math.imul(seed, 1664525) + 1013904223) | 0)
const put = (at, value) => {
  if (at >= 0 && at + 4 <= bytes.length) view.setUint32(at, value, true)
}
function compare(options = {}) {
  const {
    index = 0,
    source = 0x30000,
    frame = 0x18000,
    budget = 256,
    bias = 0,
    pattern = 'mixed',
    badBit = false,
    badDestination = false,
    change,
    interrupt,
  } = options
  const initial = names.map(() => random())
  function execute(name) {
    bytes.fill(0xa5)
    for (let i = 0; i < 32; i++)
      if (source + i >= 0 && source + i < bytes.length)
        bytes[source + i] = pattern === 'mixed' ? (i * 19) & 255 : pattern
    put(frame + 16, source - index)
    put(frame + 32, bias)
    put(frame + 28, 0x00400000)
    put(frame - 16, 0x00002000)
    env.eax.value = budget
    run.set_budget()
    names.forEach((n, i) => (env[n].value = initial[i]))
    env.ebp.value = frame
    env.ecx.value = index
    env.edx.value = alpha + index * 2 + (badDestination ? 1 : 0)
    env.esi.value = badBit ? 7 : 1 << index
    traces = []
    mutation = (count) => {
      if (count !== 1) return
      if (change === 'source') put(frame + 16, bytes.length)
      if (change === 'bias') put(frame + 32, 0x7fffffff)
      if (change === 'data') bytes[source + 31] = 255
      if (change === 'register') env.eax.value = 255
      if (interrupt) throw Error('checkpoint interrupt')
    }
    let error = null
    try {
      run[name]()
    } catch (e) {
      error = { name: e.name, message: e.message }
    }
    if (
      !error &&
      !change &&
      !interrupt &&
      !badBit &&
      !badDestination &&
      frame === 0x18000 &&
      source === 0x30000 &&
      index >= 0 &&
      index < 32
    ) {
      let blended = 0x00400000,
        opaque = 0x00002000,
        last
      for (let i = 0; i < 32 - index; i++) {
        const input = pattern === 'mixed' ? (i * 19) & 255 : pattern
        last = (input + bias) | 0
        const bit = 1 << (index + i)
        if (last > 7 && last < 248) blended |= bit
        if (last > 247) opaque |= bit
        assert.equal(view.getUint16(alpha + (index + i) * 2, true), last & 65535)
      }
      assert.equal(view.getInt32(frame + 28, true), blended)
      assert.equal(view.getInt32(frame - 16, true), opaque)
      assert.equal(env.eax.value, last)
      assert.equal(env.ecx.value, 32)
      assert.equal(env.edx.value, alpha + 64)
      assert.equal(env.esi.value, 0)
      assert.equal(env.flags.value, 0x44)
    }
    const state = snapshot()
    run.read_budget()
    return { ...state, error, traces, budget: env.eax.value }
  }
  assert.deepEqual(execute('span'), execute('plain'), JSON.stringify(options))
  cases++
}
for (let budget = 1; budget <= 161; budget++) compare({ budget })
for (const pattern of [0, 7, 8, 247, 248, 255])
  for (const index of [0, 7, 31])
    for (const budget of [3, 4, 5, 127, 160, 256]) compare({ pattern, index, budget })
for (const bias of [-0x80000000, -256, -8, -1, 1, 8, 248, 0x7fffffff]) compare({ bias })
for (const source of [
  bytes.length - 32,
  bytes.length - 31,
  bytes.length - 1,
  bytes.length,
  0xfffffffe,
  alpha - 1,
  alpha,
  alpha + 1,
  0x18000,
  0x18010,
])
  compare({ source })
for (const frame of [0, 8, bytes.length - 32, bytes.length, alpha, alpha + 16]) compare({ frame })
for (const index of [-1, 32, 33]) compare({ index })
compare({ badBit: true })
compare({ badDestination: true })
for (const change of ['source', 'bias', 'data', 'register'])
  for (const budget of [1, 2, 3, 4, 5, 6]) compare({ change, budget })
for (const budget of [1, 2, 3, 4, 5, 6]) compare({ budget, interrupt: true })
console.log(`Alpha classification: ${cases} differential cases passed`)
if (process.env.SIMS_CLASSIFY_BENCH_REPORT) {
  const { exports: fast } = new WebAssembly.Instance(module, { env: { ...env, observe() {} } })
  const iterations = 100000,
    samples = { plain: [], span: [] }
  put(0x18010, 0x30000)
  put(0x18020, 0)
  env.ebp.value = 0x18000
  for (let i = 0; i < 32; i++) bytes[0x30000 + i] = (i * 19) & 255
  for (let round = 0; round < 7; round++)
    for (const name of round % 2 ? ['span', 'plain'] : ['plain', 'span']) {
      env.eax.value = 256
      fast.set_budget()
      const start = performance.now()
      for (let i = 0; i < iterations; i++) {
        env.ecx.value = 0
        env.edx.value = alpha
        env.esi.value = 1
        fast[name]()
      }
      if (round > 0) samples[name].push(performance.now() - start)
    }
  writeFileSync(
    process.env.SIMS_CLASSIFY_BENCH_REPORT,
    JSON.stringify({ iterations, samples }, null, 2),
  )
}
