// Isolated bridge microbenchmark: node native-geometry-bench.mjs /path/to/runtime.js [calls]
// One Wasm loop invokes real runtime geometry APIs through either existing JS
// trampolines or native trampolines. This does not measure whole-game rendering.
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { resolve } from 'node:path'
import { pathToFileURL } from 'node:url'

if (!process.argv[2]) throw new Error('Pass the generated runtime.js path')
const directory = new URL('./', pathToFileURL(resolve(process.argv[2])))
const { default: init, Process } = await import(new URL('./runtime.js', directory))
await init({ module_or_path: readFileSync(new URL('./runtime_bg.wasm', directory)) })
const { ApiDispatcher } = await import(new URL('./dispatch.js', directory))
const { installSynchronousImports } = await import(new URL('./sync-imports.js', directory))
const { installNativeImports } = await import(new URL('./native-imports.js', directory))
const count = Number(process.argv[3] ?? 200000)
assert.ok(Number.isInteger(count) && count > 0 && count <= 10000000)

function unsigned(value) {
  const bytes = []
  do {
    const byte = value & 127
    value >>>= 7
    bytes.push(byte | (value ? 128 : 0))
  } while (value)
  return bytes
}
function signed(value) {
  const bytes = []
  for (;;) {
    const byte = value & 127
    value >>= 7
    const done = (value === 0 && !(byte & 64)) || (value === -1 && byte & 64)
    bytes.push(byte | (done ? 0 : 128))
    if (done) return bytes
  }
}
const name = (text) => [text.length, ...new TextEncoder().encode(text)]
const section = (id, bytes) => [id, ...unsigned(bytes.length), ...bytes]
function loop(table, esp, stack) {
  // Reset ESP before each call, exactly as independent guest call sites would.
  const body = [
    0,
    2,
    64,
    3,
    64,
    32,
    0,
    69,
    13,
    1,
    65,
    ...signed(stack),
    36,
    0,
    65,
    16,
    17,
    0,
    0,
    32,
    0,
    65,
    1,
    107,
    33,
    0,
    12,
    0,
    11,
    11,
    11,
  ]
  const module = new WebAssembly.Module(
    Uint8Array.from([
      0,
      97,
      115,
      109,
      1,
      0,
      0,
      0,
      ...section(1, [2, 0x60, 0, 0, 0x60, 1, 0x7f, 0]),
      ...section(2, [
        2,
        ...name('env'),
        ...name('table'),
        1,
        0x70,
        0,
        0,
        ...name('env'),
        ...name('esp'),
        3,
        0x7f,
        1,
      ]),
      ...section(3, [1, 1]),
      ...section(7, [1, ...name('run'), 0, 0]),
      ...section(10, [1, ...unsigned(body.length), ...body]),
    ]),
  )
  return new WebAssembly.Instance(module, { env: { table, esp } }).exports.run
}
function scenario(native, apiName) {
  const memory = new WebAssembly.Memory({ initial: 4, maximum: 64 })
  const owner = new Process(memory, 0x10000, 0x400000)
  const stack = owner.heapAlloc(64),
    pointer = owner.heapAlloc(64)
  const registers = Object.fromEntries(
    ['esp', 'eax', 'actual_return'].map((key) => [
      key,
      new WebAssembly.Global({ value: 'i32', mutable: true }, 0),
    ]),
  )
  const host = new ApiDispatcher(
    { process: owner, memory, registers },
    { yields: 0 },
    () => {},
    () => {},
  )
  const cancellation = new AbortController()
  host.threads = { signal: cancellation.signal }
  host.threadId = 7
  host.calls = 1000
  const api = JSON.parse(owner.resolve('user32.dll', apiName))
  const imports = { trap: 0, bindings: [{ label: 'user32.dll!' + apiName, api }] }
  host.bindings = imports.bindings
  const table = new WebAssembly.Table({ initial: 32, element: 'anyfunc' })
  installSynchronousImports(table, imports, (slot) => host.dispatchSync(slot))
  if (native) assert.equal(installNativeImports(table, imports, host), 1)
  const args = {
    OffsetRect: [pointer, 1, -1],
    InflateRect: [pointer, 1, 1],
    IntersectRect: [pointer + 32, pointer, pointer + 16],
    UnionRect: [pointer + 32, pointer, pointer + 16],
    IsRectEmpty: [pointer],
    PtInRect: [pointer, 4, 5],
  }[apiName]
  const view = new DataView(memory.buffer)
  for (const [i, value] of [0x12345678, ...args].entries())
    view.setUint32(stack + i * 4, value, true)
  for (const [i, value] of [0, 0, 10, 10, 5, -5, 15, 8].entries())
    view.setUint32(pointer + i * 4, value, true)
  return {
    run: loop(table, registers.esp, stack),
    state: () => ({
      registers: Object.fromEntries(
        Object.entries(registers).map(([key, value]) => [key, value.value >>> 0]),
      ),
      pixels: [...owner.read(pointer, 64)],
      calls: host.calls,
    }),
    close: () => {
      cancellation.abort(new Error('Benchmark closed'))
      owner.free()
    },
  }
}
const median = (values) => {
  const sorted = values.slice(1).sort((a, b) => a - b)
  return (sorted[2] + sorted[3]) / 2
}
const cases = {}
for (const api of [
  'OffsetRect',
  'InflateRect',
  'IntersectRect',
  'UnionRect',
  'IsRectEmpty',
  'PtInRect',
]) {
  const hosts = [scenario(false, api), scenario(true, api)]
  try {
    const samples = [[], []]
    for (let round = 0; round < 7; round++)
      for (let step = 0; step < 2; step++) {
        const index = (round + step) % 2,
          start = performance.now()
        hosts[index].run(count)
        samples[index].push(performance.now() - start)
      }
    assert.deepEqual(hosts[0].state(), hosts[1].state(), api + ' output/state parity')
    const before = median(samples[0]),
      after = median(samples[1])
    cases[api] = {
      baseline_ms: samples[0],
      native_ms: samples[1],
      baseline_median_ms: before,
      native_median_ms: after,
      reduction_percent: (100 * (before - after)) / before,
    }
  } finally {
    hosts.forEach((host) => host.close())
  }
}
console.log(
  JSON.stringify(
    {
      kind: 'isolated real-runtime Wasm-loop bridge microbenchmark',
      engine: process.version,
      count,
      rounds: 7,
      discard_first_sample: true,
      cases,
    },
    null,
    2,
  ),
)
