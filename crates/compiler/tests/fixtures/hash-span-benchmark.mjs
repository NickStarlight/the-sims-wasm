// SIMS_HASH_BENCH_MODULE="$PWD/.tools/hash-span.wasm" cargo test -p sims-compiler --lib export_hash_span_benchmark -- --ignored
// node crates/compiler/tests/fixtures/hash-span-benchmark.mjs .tools/hash-span.wasm
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
const env = { memory: new WebAssembly.Memory({ initial: 2 }), observe() {} }
for (const name of names) env[name] = new WebAssembly.Global({ value: 'i32', mutable: true }, 0)
const { exports: run } = new WebAssembly.Instance(
  new WebAssembly.Module(readFileSync(process.argv[2])),
  { env },
)
const bytes = new Uint8Array(env.memory.buffer),
  view = new DataView(bytes.buffer)
view.setUint32(0x2000, 0, true)
view.setUint32(0x2004, 0x4000, true)
const median = (a) => a.slice().sort((a, b) => a - b)[Math.floor(a.length / 2)]
const result = {
  engine: process.version,
  targetBytesPerSample: 2000000,
  method:
    'Wasm caller loop; alternate order; discard first two warmups; median9paired samples; exact result parity.',
  cases: [],
}
for (const length of [8, 24, 64, 256]) {
  bytes.fill(0, 0x4000, 0x4200)
  for (let i = 0; i < length; i++) bytes[0x4000 + i] = 1 + ((i * 73) % 255)
  const callsPerSample = Math.max(4000, Math.floor(result.targetBytesPerSample / length))
  const rounds = { plain: [], fast: [] }
  for (let round = 0; round < 11; round++) {
    const out = {}
    for (const name of round % 2 ? ['fast', 'plain'] : ['plain', 'fast']) {
      env.eax.value = 256
      run.set_budget()
      const start = performance.now()
      run['bench_' + name](callsPerSample)
      const ms = performance.now() - start
      out[name] = env.eax.value >>> 0
      if (round >= 2) rounds[name].push(ms)
    }
    assert.equal(out.plain, out.fast)
  }
  const before = median(rounds.plain),
    after = median(rounds.fast)
  result.cases.push({
    length,
    callsPerSample,
    plainMs: before,
    fastMs: after,
    speedup: before / after,
    rounds,
  })
}
console.log(JSON.stringify(result, null, 2))
