// Generate with SIMS_COMPOSITOR_BENCH_MODULE=<file> cargo test -p sims-compiler --lib export_compositor_span_benchmark -- --ignored
// Then: node compositor-span-benchmark.mjs <file>
import { readFileSync } from 'node:fs'
import { createHash } from 'node:crypto'
import assert from 'node:assert/strict'

const wasm = readFileSync(process.argv[2])
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
let checkpoints = 0
const env = {
  memory,
  observe() {
    checkpoints++
  },
}
for (const name of names) env[name] = new WebAssembly.Global({ value: 'i32', mutable: true }, 0)
const { exports } = new WebAssembly.Instance(new WebAssembly.Module(wasm), { env })
const bytes = new Uint8Array(memory.buffer),
  view = new DataView(memory.buffer)
for (let i = 0; i < 32768; i++) bytes[0x6e5e5c + i] = (i * 73 + (i >>> 7) * 19) & 255
for (let i = 0; i < 32; i++) {
  view.setUint16(0x6e5c0c + 2 * i, (i * 19 + 7) & 255, true)
  view.setUint16(0x6e5c4c + 2 * i, (i * 9173) & 65535, true)
  view.setUint16(0x6e5d0c + 2 * i, (i * 3147 + 99) & 65535, true)
}
function run(name, count) {
  bytes.fill(0xa5, 0x17f00, 0x20100)
  checkpoints = 0
  env.eax.value = 256
  exports.set_budget()
  names.forEach((name, index) => (env[name].value = index * 317))
  env.esp.value = 0x17fc0
  env.ebp.value = 0x18000
  env.edi.value = 31
  const start = performance.now()
  exports['bench_' + name](count)
  const elapsed = performance.now() - start
  const state = {
    checkpoints,
    registers: names.map((name) => env[name].value),
    memory: createHash('sha256').update(bytes).digest('hex'),
  }
  exports.read_budget()
  state.budget = env.eax.value
  return { elapsed, state }
}
const count = 200000
const times = { plain: [], span: [] }
run('plain', 5000)
run('span', 5000)
for (let round = 0; round < 8; round++) {
  const results = {}
  for (const name of round % 2 ? ['span', 'plain'] : ['plain', 'span']) {
    results[name] = run(name, count)
    times[name].push(results[name].elapsed)
  }
  assert.deepEqual(results.span.state, results.plain.state)
}
const median = (values) => {
  const ordered = values.slice(1).sort((a, b) => a - b)
  return ordered[(ordered.length - 1) / 2]
}
const before = median(times.plain),
  after = median(times.span)
console.log(
  JSON.stringify(
    {
      engine: process.version,
      moduleSha256: createHash('sha256').update(wasm).digest('hex'),
      callsPerRound: count,
      pixelsPerCall: 32,
      rounds: 8,
      warmupRoundDiscarded: true,
      times,
      baselineMedianMs: before,
      spanMedianMs: after,
      speedup: before / after,
      reductionPercent: 100 * (1 - after / before),
      parity:
        'All guest registers, checkpoint counts, remaining budget and complete8MiB memory equal after every timed pair.',
    },
    null,
    2,
  ),
)
