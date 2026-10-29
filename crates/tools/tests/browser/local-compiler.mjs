// Loader boundary tests. Compiler execution and IndexedDB have separate real-Wasm/IDB tests.
import assert from 'node:assert/strict'
import { File } from 'node:buffer'
import { createHash } from 'node:crypto'
import { mkdtemp, readFile, rm, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join, resolve } from 'node:path'
import { pathToFileURL } from 'node:url'

const source = process.argv[2]
  ? pathToFileURL(resolve(process.argv[2]))
  : new URL('../../../../web/game.js', import.meta.url)
const gameSource = await readFile(source, 'utf8')
const wasm = Uint8Array.of(0, 97, 115, 109, 1, 0, 0, 0)
const packed = Uint8Array.of(11, 12, 13, 21, 22, 23, 31, 32, 33)
const commandLine = 'C:\\Sims\\Sims.exe -skip_intro'
const sha = (bytes) => createHash('sha256').update(bytes).digest('hex')
const metadata = {
  funcAddrs: [0x400010, 0x10000010, 0x10100010],
  funcCount: 3,
  tableAddr: 0xc00000,
  trampBase: 3,
  entry: 0x400010,
  commandLineAddr: 0xd00000,
}
const plan = {
  images: [0x400000, 0x10000000, 0x10100000].map((base, index) => ({
    base,
    preferredBase: base,
    sizeOfImage: 0x1000,
    entry: base + 16,
    name: null,
    isDll: index !== 0,
    imports: [],
    exports: [],
  })),
  spans: [0x400000, 0x10000000, 0x10100000].map((address, index) => ({
    address,
    offset: index * 3,
    length: 3,
    writable: false,
  })),
}

const compilerSource = `
const fixture = globalThis.__localCompilerFixture
fixture.evaluations++
fixture.step('module')
export default async function init(options) {
  fixture.checkInit(options)
  fixture.init++
  if (fixture.forbidCompiler) throw new Error('cached launch initialized compiler')
  fixture.step('init')
}
export function completeProfileJson() {
  fixture.step('profile')
  return JSON.stringify(fixture.profile)
}
export class Compiler {
  constructor(bytes) {
    fixture.step('constructor')
    fixture.constructed.push([...bytes])
  }
  addModule(bytes, base) {
    fixture.modules.push({bytes: [...bytes], base})
    fixture.step('addModule')
  }
  compile(roots) {
    fixture.roots.push([...roots])
    fixture.step('compile')
    fixture.artifacts++
    return {
      takeWasm() { fixture.step('wasm'); return fixture.wasm.slice() },
      runtimeMetadataJson() { fixture.step('metadata'); return fixture.metadataText ?? JSON.stringify(fixture.metadata) },
      free() { fixture.artifactFreed++; fixture.step('artifact.free') },
    }
  }
  loadPlanJson() { fixture.step('plan'); return JSON.stringify(fixture.plan) }
  loadBytes() { fixture.step('loadBytes'); return fixture.packed.slice() }
  free() { fixture.compilerFreed++; fixture.step('compiler.free') }
}
`

const cacheSource = `
export async function openCompiledCache() {
  const f = globalThis.__localCompilerFixture
  f.opened++
  f.step('cache.open')
  return {
    async get(key) {
      f.guard(); f.gets.push(key); f.step('cache.get')
      return structuredClone(f.records.get(key))
    },
    async put(key, value) {
      f.guard(); f.puts.push({key, value: structuredClone(value)}); f.step('cache.put')
      if (f.unavailable) return false
      f.records.clear(); f.records.set(key, structuredClone(value)); return true
    },
    async delete(key) {
      f.guard(); f.deletes.push(key); f.step('cache.delete')
      return f.records.delete(key)
    },
    close() { f.closed++; f.events.push('cache.close') },
  }
}
`

function binary(state, path, bytes) {
  const file = new File([Uint8Array.from(bytes)], path)
  const arrayBuffer = file.arrayBuffer.bind(file)
  Object.defineProperty(file, 'arrayBuffer', {
    value() {
      state.reads.push(path)
      if (state.readFailure === path) throw new Error('local binary read failed')
      return arrayBuffer()
    },
  })
  return { path, file }
}

function asset(path) {
  const file = new File(['private asset'], path)
  Object.defineProperty(file, 'arrayBuffer', {
    value() {
      throw new Error(`Loader must not read asset ${path}`)
    },
  })
  return { path, file }
}

function fixture() {
  const state = {
    executable: [77, 90, 1, 2, 3],
    evaluations: 0,
    init: 0,
    artifacts: 0,
    artifactFreed: 0,
    compilerFreed: 0,
    opened: 0,
    closed: 0,
    locked: false,
    useLocks: true,
    events: [],
    constructed: [],
    modules: [],
    roots: [],
    reads: [],
    gets: [],
    puts: [],
    deletes: [],
    fetches: [],
    fetchOptions: [],
    createdUrls: [],
    revokedUrls: [],
    initInputs: [],
    compilerJavascript: compilerSource,
    compilerWasm: wasm.slice(),
    locks: [],
    phases: [],
    progress: [],
    records: new Map(),
    wasm: wasm.slice(),
    packed: packed.slice(),
    metadata: structuredClone(metadata),
    plan: structuredClone(plan),
    step(name) {
      this.events.push(name)
      if (this.failures?.has(name)) throw this.failures.get(name)
      if (this.failure === name) throw new Error(`fixture failure: ${name}`)
    },
    checkInit(options) {
      assert.deepEqual(Object.keys(options), ['module_or_path'])
      assert.ok(options.module_or_path instanceof Uint8Array)
      assert.deepEqual(
        options.module_or_path,
        this.compilerWasm,
        'initialize only verified compiler bytes',
      )
      assert.deepEqual(
        this.revokedUrls,
        this.createdUrls,
        'module URL is released before initialization',
      )
      this.initInputs.push(options.module_or_path.slice())
    },
    guard() {
      assert.ok(!this.useLocks || this.locked, 'cache access stays inside the compilation lock')
    },
  }
  state.profile = {
    sha256: sha(Uint8Array.from(state.executable)),
    roots: [0x400010, 0x10000010, 0x10100010],
    modules: [
      { name: 'gimex.dll', base: 0x10000000 },
      { name: 'ijl10.dll', base: 0x10100000 },
    ],
    requiredFiles: ['ExpansionPack7/ExpansionPack7.far'],
    initialRegistry: {
      'hkey_local_machine\\software\\maxis\\the sims': { SIMS_SKU: { t: 4, d: 'AQAAAA==' } },
    },
  }
  const { sha256, modules, requiredFiles } = structuredClone(state.profile)
  state.build = {
    version: 1,
    compilerId: sha(Buffer.concat([Buffer.from(state.compilerJavascript), state.compilerWasm])),
    wasmSha256: sha(wasm),
    profile: { sha256, modules, requiredFiles },
  }
  state.files = [
    asset('GAMEDATA\\Objects\\Objects.far'),
    binary(state, 'SIMS.EXE', state.executable),
    asset('userdata/NEIGHBORHOOD.iff'),
    binary(state, 'GIMEX.DLL', [4, 5, 6, 7]),
    asset('expansionpack7/EXPANSIONPACK7.far'),
    binary(state, 'IJL10.dll', [8, 9, 10, 11]),
    asset('UserData/Characters/User00000.iff'),
  ]
  return state
}

async function keyFor(state, files = state.files) {
  const inputs = []
  for (const path of ['Sims.exe', ...state.profile.modules.map(({ name }) => name)]) {
    const file = files.find((entry) => entry.path.toLowerCase() === path.toLowerCase()).file
    inputs.push({ path, sha256: sha(new Uint8Array(await File.prototype.arrayBuffer.call(file))) })
  }
  return JSON.stringify({ version: 1, compilerId: state.build.compilerId, inputs, commandLine })
}

function cachedValue(state) {
  const value = {
    version: 1,
    wasm: wasm.slice(),
    imageBytes: packed.slice(),
    metadata: structuredClone(metadata),
    plan: structuredClone(plan),
    clientId: state.profile.sha256,
    commandLine,
    initialRegistry: structuredClone(state.profile.initialRegistry),
  }
  for (const [index, name] of ['Sims.exe', 'gimex.dll', 'ijl10.dll'].entries())
    value.plan.images[index].path = `C:\\Sims\\${name}`
  return value
}

function assertProgram(program, state, files = state.files) {
  assert.strictEqual(
    program.files,
    files,
    'reattach the exact selected File handles without replacing or appending binaries',
  )
  assert.equal(program.files.length, files.length)
  assert.ok(!('module' in program), 'terminate the compiler worker before JIT compilation')
  assert.deepEqual(program.wasm, state.wasm)
  assert.ok(WebAssembly.validate(program.wasm))
  assert.equal(program.clientId, state.profile.sha256)
  assert.equal(program.commandLine, commandLine)
  assert.deepEqual(program.metadata, metadata)
  assert.deepEqual(program.initialRegistry, state.profile.initialRegistry)
  assert.deepEqual(
    program.imageBytes,
    packed,
    'mapped spans only: no appended raw EXE/DLL payloads',
  )
  assert.deepEqual(
    program.plan.images.map((image) => image.path),
    ['C:\\Sims\\Sims.exe', 'C:\\Sims\\gimex.dll', 'C:\\Sims\\ijl10.dll'],
  )
  assert.equal(
    program.plan.spans.reduce((size, span) => size + span.length, 0),
    packed.length,
  )
}

let passed = 0
let moduleSerial = 0
async function check(name, body) {
  const directory = await mkdtemp(join(tmpdir(), 'sims-local-compiler-'))
  const saved = new Map(
    ['fetch', 'navigator', 'Blob', '__localCompilerFixture'].map((key) => [
      key,
      Object.getOwnPropertyDescriptor(globalThis, key),
    ]),
  )
  const urlMethods = new Map(
    ['createObjectURL', 'revokeObjectURL'].map((key) => [
      key,
      Object.getOwnPropertyDescriptor(URL, key),
    ]),
  )
  const NativeBlob = globalThis.Blob
  const compile = WebAssembly.compile
  const state = fixture()
  try {
    WebAssembly.compile = () => {
      assert.fail('JIT the game only after terminating the compiler worker')
    }
    Object.defineProperty(globalThis, 'Blob', {
      configurable: true,
      value: class FixtureBlob extends NativeBlob {
        constructor(parts, options) {
          super(parts, options)
          this.parts = parts
        }
      },
    })
    Object.defineProperty(URL, 'createObjectURL', {
      configurable: true,
      value(blob) {
        const bytes = Buffer.concat(
          blob.parts.map((part) =>
            typeof part === 'string'
              ? Buffer.from(part)
              : ArrayBuffer.isView(part)
                ? Buffer.from(part.buffer, part.byteOffset, part.byteLength)
                : Buffer.from(part),
          ),
        )
        const url = `data:text/javascript;base64,${bytes.toString('base64')}#fixture-${++moduleSerial}`
        state.createdUrls.push(url)
        return url
      },
    })
    Object.defineProperty(URL, 'revokeObjectURL', {
      configurable: true,
      value(url) {
        assert.ok(state.createdUrls.includes(url))
        assert.ok(!state.revokedUrls.includes(url), 'revoke each compiler URL once')
        state.revokedUrls.push(url)
      },
    })
    Object.defineProperty(globalThis, '__localCompilerFixture', {
      configurable: true,
      value: state,
    })
    Object.defineProperty(globalThis, 'fetch', {
      configurable: true,
      value: async (url, options) => {
        state.fetches.push(url)
        state.fetchOptions.push({ url, options })
        assert.ok(
          ['./compiler-build.json', './compiler.js', './compiler_bg.wasm'].includes(url),
          'the loader must never fetch prepared game bytes',
        )
        state.step('fetch')
        if (url === './compiler-build.json')
          return new Response(JSON.stringify(state.build), { status: state.httpStatus ?? 200 })
        assert.equal(options?.cache, 'reload', 'cold compiler downloads bypass stale HTTP cache')
        state.step(url === './compiler.js' ? 'fetch.glue' : 'fetch.wasm')
        return new Response(
          url === './compiler.js' ? state.compilerJavascript : state.compilerWasm,
          { status: state.compilerHttpStatus ?? 200 },
        )
      },
    })
    Object.defineProperty(globalThis, 'navigator', {
      configurable: true,
      get() {
        return state.useLocks
          ? {
              locks: {
                async request(name, callback) {
                  state.locks.push(name)
                  assert.equal(name, 'sims-web:compilation')
                  assert.equal(state.locked, false)
                  state.step('lock')
                  state.locked = true
                  try {
                    return await callback()
                  } finally {
                    state.locked = false
                  }
                },
              },
            }
          : {}
      },
    })
    await Promise.all([
      writeFile(join(directory, 'package.json'), '{"type":"module"}'),
      writeFile(join(directory, 'game.js'), gameSource),
      writeFile(join(directory, 'compiled-cache.js'), cacheSource),
    ])
    const { loadGame } = await import(pathToFileURL(join(directory, 'game.js')).href)
    state.load = (files = state.files) =>
      loadGame(
        files,
        (phase) => {
          assert.ok(Number.isFinite(phase.atMs))
          state.phases.push(phase.name)
        },
        (message) => state.progress.push(message),
      )
    await body(state)
    assert.equal(state.locked, false, 'release the compilation lock')
    assert.equal(state.closed, state.opened, 'close every opened cache')
    assert.deepEqual(
      state.revokedUrls,
      state.createdUrls,
      'release every compiler module URL, including failed imports/init',
    )
    passed++
  } catch (error) {
    throw new Error(`${name}: ${error.message}`, { cause: error })
  } finally {
    WebAssembly.compile = compile
    for (const [key, descriptor] of urlMethods) {
      if (descriptor) Object.defineProperty(URL, key, descriptor)
      else delete URL[key]
    }
    for (const [key, descriptor] of saved) {
      if (descriptor) Object.defineProperty(globalThis, key, descriptor)
      else delete globalThis[key]
    }
    await rm(directory, { recursive: true, force: true })
  }
}

await check('cold local compilation', async (f) => {
  assertProgram(await f.load(), f)
  assert.deepEqual(f.constructed, [f.executable])
  assert.deepEqual(f.modules, [
    { bytes: [4, 5, 6, 7], base: 0x10000000 },
    { bytes: [8, 9, 10, 11], base: 0x10100000 },
  ])
  assert.deepEqual(f.roots, [f.profile.roots])
  assert.equal(f.init, 1)
  assert.equal(f.compilerFreed, 1)
  assert.equal(f.artifactFreed, 1)
  assert.deepEqual(
    f.events.filter((event) => event.endsWith('.free')),
    ['artifact.free', 'compiler.free'],
  )
  assert.equal(f.puts.length, 1)
  assert.equal(f.puts[0].key, await keyFor(f))
  assert.ok(!('files' in f.puts[0].value), 'never persist File objects or raw binary handles')
  assert.deepEqual(
    f.fetches.sort(),
    ['./compiler-build.json', './compiler.js', './compiler_bg.wasm'].sort(),
  )
  assert.equal(f.initInputs.length, 1)
  assert.equal(f.createdUrls.length, 1)
  assert.deepEqual(f.reads.sort(), ['GIMEX.DLL', 'IJL10.dll', 'SIMS.EXE'])
  assert.deepEqual(f.phases, [
    'local-files-check-start',
    'local-compile-cache-miss',
    'local-compile-start',
    'local-compile-done',
    'client-ready',
  ])
})

await check('warm hit never imports or initializes compiler', async (f) => {
  f.records.set(await keyFor(f), cachedValue(f))
  f.forbidCompiler = true
  assertProgram(await f.load(), f)
  assert.equal(f.evaluations, 0)
  assert.equal(f.init, 0)
  assert.deepEqual(f.fetches, ['./compiler-build.json'])
  assert.equal(f.createdUrls.length, 0)
  assert.equal(f.puts.length, 0)
  assert.equal(f.constructed.length, 0)
  assert.deepEqual(f.phases, ['local-files-check-start', 'local-compile-cache-hit', 'client-ready'])
})

await check('cold and cached game buffers survive both worker transfers', async (f) => {
  for (const launch of ['cold', 'cached']) {
    const prepared = await f.load()
    const main = structuredClone(prepared, {
      transfer: [prepared.wasm.buffer, prepared.imageBytes.buffer],
    })
    assert.equal(prepared.wasm.byteLength, 0, `${launch}: no retained compiler-worker copy`)
    assert.equal(prepared.imageBytes.byteLength, 0)
    const runtime = structuredClone(main, { transfer: [main.wasm.buffer, main.imageBytes.buffer] })
    assert.equal(main.wasm.byteLength, 0, `${launch}: no retained main-thread copy`)
    assert.equal(main.imageBytes.byteLength, 0)
    assertProgram(runtime, f, runtime.files)
    const saved = f.records.get(await keyFor(f))
    assert.deepEqual(saved.wasm, wasm, 'persisted artifact remains usable after transfer')
    assert.deepEqual(saved.imageBytes, packed)
  }
  assert.equal(f.init, 1, 'the second transfer comes from the compilation cache')
})

await check('DLL content and compiler identity invalidate cache', async (f) => {
  assertProgram(await f.load(), f)
  assertProgram(await f.load(), f)
  assert.equal(f.init, 1, 'unchanged inputs hit cache')
  for (const name of ['GIMEX.DLL', 'IJL10.dll']) {
    f.files = f.files.map((entry) =>
      entry.path === name ? binary(f, name, [99, 98, name.length]) : entry,
    )
    assertProgram(await f.load(), f)
    assert.equal(f.puts.at(-1).key, await keyFor(f))
  }
  f.compilerJavascript += '\n// compiler build v2\n'
  f.build.compilerId = sha(Buffer.concat([Buffer.from(f.compilerJavascript), f.compilerWasm]))
  assertProgram(await f.load(), f)
  assert.equal(f.init, 4)
  assert.equal(new Set(f.puts.map(({ key }) => key)).size, 4)
  for (const { key } of f.puts) assert.equal(JSON.parse(key).commandLine, commandLine)
})

await check('verified compiler Wasm update invalidates cached game', async (f) => {
  assertProgram(await f.load(), f)
  f.compilerWasm = Uint8Array.from([...wasm, 0, 1, 0])
  f.build.wasmSha256 = sha(f.compilerWasm)
  f.build.compilerId = sha(Buffer.concat([Buffer.from(f.compilerJavascript), f.compilerWasm]))
  assertProgram(await f.load(), f)
  assert.equal(f.init, 2)
  assert.equal(f.puts.length, 2)
  assert.deepEqual(f.initInputs, [wasm, f.compilerWasm])
})

for (const mismatch of ['glue', 'wasm', 'compilerId', 'wasmSha256', 'wasm-with-new-digest']) {
  await check(`reject mixed compiler bytes: ${mismatch}`, async (f) => {
    if (mismatch === 'glue') f.compilerJavascript += '\n// stale compiler glue\n'
    if (mismatch === 'wasm' || mismatch === 'wasm-with-new-digest')
      f.compilerWasm = Uint8Array.from([...wasm, 0, 1, 0])
    if (mismatch === 'compilerId') f.build.compilerId = sha('other compiler')
    if (mismatch === 'wasmSha256') f.build.wasmSha256 = sha('other Wasm')
    if (mismatch === 'wasm-with-new-digest') f.build.wasmSha256 = sha(f.compilerWasm)
    await assert.rejects(f.load(), /Compiler files do not match/)
    assert.equal(f.evaluations, 0, 'reject before evaluating unverified code')
    assert.equal(f.init, 0)
    assert.equal(f.createdUrls.length, 0)
    assert.equal(f.puts.length, 0)
    assert.equal(f.records.size, 0)
  })
}

await check('verified but invalid module syntax releases URL', async (f) => {
  f.compilerJavascript = 'export const invalid = ;'
  f.build.compilerId = sha(Buffer.concat([Buffer.from(f.compilerJavascript), f.compilerWasm]))
  await assert.rejects(f.load(), SyntaxError)
  assert.equal(f.createdUrls.length, 1)
  assert.equal(f.init, 0)
  assert.equal(f.puts.length, 0)
})

await check('compiler file HTTP failure initializes nothing', async (f) => {
  f.compilerHttpStatus = 404
  await assert.rejects(f.load(), /compiler could not be loaded/)
  assert.equal(f.createdUrls.length, 0)
  assert.equal(f.init, 0)
  assert.equal(f.puts.length, 0)
})

for (const missing of [
  'SIMS.EXE',
  'GAMEDATA\\Objects\\Objects.far',
  'userdata/NEIGHBORHOOD.iff',
  'GIMEX.DLL',
  'IJL10.dll',
  'expansionpack7/EXPANSIONPACK7.far',
]) {
  await check(`missing selected file ${missing}`, async (f) => {
    await assert.rejects(
      f.load(f.files.filter(({ path }) => path !== missing)),
      /Choose|missing|requires/,
    )
    assert.equal(f.init, 0)
    assert.equal(f.opened, 0)
    assert.equal(f.reads.length, 0, 'validate the complete selection before reading binaries')
    if (
      ['SIMS.EXE', 'GAMEDATA\\Objects\\Objects.far', 'userdata/NEIGHBORHOOD.iff'].includes(missing)
    )
      assert.equal(f.fetches.length, 0)
  })
}

await check('unsupported executable fails before compiler/cache', async (f) => {
  f.files = f.files.map((entry) =>
    entry.path === 'SIMS.EXE' ? binary(f, 'SIMS.EXE', [77, 90, 99]) : entry,
  )
  await assert.rejects(f.load(), /not the supported Complete Collection/)
  assert.equal(f.init, 0)
  assert.equal(f.opened, 0)
})

await check('corrupt cached Wasm is removed and recompiled', async (f) => {
  const key = await keyFor(f),
    value = cachedValue(f)
  value.wasm = Uint8Array.of(0, 1, 2, 3, 4, 5, 6, 7)
  f.records.set(key, value)
  assertProgram(await f.load(), f)
  assert.deepEqual(f.deletes, [key])
  assert.equal(f.init, 1)
  assert.equal(f.puts.length, 1)
  assert.deepEqual(f.records.get(key).wasm, wasm)
  assert.ok(!f.phases.includes('local-compile-cache-hit'))
})

for (const field of ['clientId', 'commandLine']) {
  await check(`cached ${field} mismatch is recompiled`, async (f) => {
    const key = await keyFor(f),
      value = cachedValue(f)
    value[field] = field === 'clientId' ? sha('other game') : 'C:\\Sims\\Sims.exe'
    f.records.set(key, value)
    assertProgram(await f.load(), f)
    assert.equal(f.init, 1)
    assert.equal(f.puts.length, 1)
    assert.equal(f.records.get(key)[field], cachedValue(f)[field])
    assert.ok(!f.phases.includes('local-compile-cache-hit'))
  })
}

for (const field of ['sha256', 'modules-length', 'module-name', 'module-base', 'requiredFiles']) {
  await check(`full compiler profile mismatch: ${field}`, async (f) => {
    if (field === 'sha256') f.profile.sha256 = sha('different executable')
    if (field === 'modules-length') f.profile.modules.pop()
    if (field === 'module-name') f.profile.modules[0].name = 'other.dll'
    if (field === 'module-base') f.profile.modules[0].base++
    if (field === 'requiredFiles') f.profile.requiredFiles.push('extra.far')
    await assert.rejects(f.load(), /Compiler files do not match/)
    assert.equal(f.constructed.length, 0)
    assert.equal(f.puts.length, 0)
  })
}

for (const failure of [
  'fetch.glue',
  'fetch.wasm',
  'module',
  'init',
  'constructor',
  'addModule',
  'compile',
  'plan',
  'wasm',
  'metadata',
  'loadBytes',
  'cache.get',
  'cache.put',
  'lock',
]) {
  await check(`cleanup after ${failure} failure`, async (f) => {
    f.failure = failure
    await assert.rejects(f.load(), new RegExp(`fixture failure: ${failure}`))
    assert.equal(f.compilerFreed, f.constructed.length)
    assert.equal(f.artifactFreed, f.artifacts)
    assert.equal(f.records.size, 0)
    assert.ok(!f.phases.includes('client-ready'))
  })
}

await check('invalid newly compiled Wasm frees both owners', async (f) => {
  f.wasm = Uint8Array.of(1, 2, 3)
  await assert.rejects(f.load(), WebAssembly.CompileError)
  assert.equal(f.compilerFreed, 1)
  assert.equal(f.artifactFreed, 1)
  assert.equal(f.puts.length, 0)
})

for (const stage of ['compile', 'plan', 'wasm', 'metadata', 'loadBytes']) {
  await check(`Wasm trap during ${stage} survives poisoned owner cleanup`, async (f) => {
    const trap = new WebAssembly.RuntimeError('unreachable executed during compilation')
    const borrowed = new Error('attempted to take ownership of Rust value while it was borrowed')
    f.failures = new Map([
      [stage, trap],
      ['artifact.free', borrowed],
      ['compiler.free', borrowed],
    ])
    await assert.rejects(f.load(), (error) => error === trap)
    assert.equal(f.compilerFreed, 1)
    assert.equal(f.artifactFreed, stage === 'compile' ? 0 : 1)
    assert.equal(f.puts.length, 0)
    assert.ok(!f.phases.includes('local-compile-done'))
    assert.ok(!f.phases.includes('client-ready'))
  })
}

for (const stage of ['artifact.free', 'compiler.free']) {
  await check(`cleanup failure in ${stage} still releases both owners and rejects`, async (f) => {
    f.failure = stage
    await assert.rejects(f.load(), new RegExp(`fixture failure: ${stage}`))
    assert.equal(f.compilerFreed, 1)
    assert.equal(f.artifactFreed, 1)
    assert.equal(f.puts.length, 0)
    assert.ok(!f.phases.includes('client-ready'))
  })
}

await check('invalid metadata frees both owners', async (f) => {
  f.metadataText = '{'
  await assert.rejects(f.load(), SyntaxError)
  assert.equal(f.compilerFreed, 1)
  assert.equal(f.artifactFreed, 1)
  assert.equal(f.puts.length, 0)
})

await check('local file read failure opens no cache', async (f) => {
  f.readFailure = 'IJL10.dll'
  await assert.rejects(f.load(), /local binary read failed/)
  assert.equal(f.opened, 0)
  assert.equal(f.init, 0)
})

await check('cache unavailability and missing Web Locks still launch', async (f) => {
  f.useLocks = false
  f.unavailable = true
  assertProgram(await f.load(), f)
  assert.equal(f.locks.length, 0)
  assert.equal(f.records.size, 0)
})

for (const [name, change, expected] of [
  [
    'download',
    (f) => {
      f.httpStatus = 404
    },
    /compiler could not be loaded/,
  ],
  [
    'version',
    (f) => {
      f.build.version = 2
    },
    /Unsupported compiler configuration/,
  ],
  [
    'compiler identity',
    (f) => {
      f.build.compilerId = 'bad'
    },
    /Invalid game identity/,
  ],
  [
    'Wasm identity',
    (f) => {
      f.build.wasmSha256 = 'bad'
    },
    /Invalid game identity/,
  ],
  [
    'profile identity',
    (f) => {
      f.build.profile.sha256 = 'bad'
    },
    /Invalid game identity/,
  ],
  [
    'module list',
    (f) => {
      f.build.profile.modules = null
    },
    /Unsupported compiler configuration/,
  ],
]) {
  await check(`invalid build ${name}`, async (f) => {
    change(f)
    await assert.rejects(f.load(), expected)
    assert.equal(f.init, 0)
    assert.equal(f.opened, 0)
    assert.equal(f.reads.length, 0)
  })
}

console.log(`Local compiler loader: ${passed} cold/cache/input/ownership/error cases passed`)
