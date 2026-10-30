// SIMS_GEOMETRY_BENCH_MODULE="$PWD/.tools/geometry.wasm" cargo test -p sims-compiler --lib export_geometry_benchmark -- --ignored
// node crates/compiler/tests/fixtures/geometry-benchmark.mjs .tools/geometry.wasm
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { createHash } from 'node:crypto'

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
let checkpoints = 0
const env = {
  memory: new WebAssembly.Memory({ initial: 128 }),
  observe() {
    checkpoints++
  },
}
for (const name of names) env[name] = new WebAssembly.Global({ value: 'i32', mutable: true }, 0)
const { exports: run } = new WebAssembly.Instance(new WebAssembly.Module(wasm), { env })
const bytes = new Uint8Array(env.memory.buffer),
  view = new DataView(bytes.buffer)
function execute(suffix, count) {
  bytes.fill(0)
  for (let i = 0; i < 32768; i++) bytes[0x6de81c + i] = (i * 73 + (i >>> 7)) & 63
  bytes[0x4000] = 17
  bytes[0x4001] = 23
  for (const [address, value] of [
    [0x2000, 0],
    [0x2004, 1],
  ])
    view.setInt32(address, value, true)
  env.eax.value = 256
  run.set_budget()
  names.forEach((name, i) => {
    env[name].value = i * 317
  })
  env.resume_next.value = 0
  checkpoints = 0
  const start = performance.now()
  run[`bench_transform_${suffix}`](count)
  const ms = performance.now() - start
  const state = {
    registers: names.map((name) => env[name].value),
    memory: createHash('sha256').update(bytes).digest('hex'),
    checkpoints,
  }
  run.read_budget()
  state.budget = env.eax.value
  return { ms, state }
}
const count = 1000000
const times = { plain: [], fast: [] }
execute('plain', 10000)
execute('fast', 10000)
for (let round = 0; round < 9; round++) {
  const pair = {}
  for (const suffix of round % 2 ? ['fast', 'plain'] : ['plain', 'fast']) {
    pair[suffix] = execute(suffix, count)
    times[suffix].push(pair[suffix].ms)
  }
  assert.deepEqual(pair.fast.state, pair.plain.state)
}
const median = (values) => values.slice(1).sort((a, b) => a - b)[3]
const plain = median(times.plain),
  fast = median(times.fast)
const results = [
  {
    kind: 'transform',
    count,
    times,
    plainMedianMs: plain,
    fastMedianMs: fast,
    speedup: plain / fast,
    reductionPercent: 100 * (1 - fast / plain),
  },
]
console.log(
  JSON.stringify(
    {
      engine: process.version,
      moduleSha256: createHash('sha256').update(wasm).digest('hex'),
      rounds: 9,
      discardedWarmupRounds: 1,
      parity:
        'Complete memory, registers, checkpoint counts and remaining budgets match after every timed pair.',
      results,
    },
    null,
    2,
  ),
)
