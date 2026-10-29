import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'

export async function compilerMetadataSmoke(directory) {
  const {
    default: init,
    Compiler,
    completeProfileJson,
  } = await import(new URL('./compiler.js', directory))
  await init({ module_or_path: readFileSync(new URL('./compiler_bg.wasm', directory)) })
  const profileText = completeProfileJson()
  const profile = JSON.parse(profileText)
  assert.deepEqual(Object.keys(profile).sort(), [
    'initialRegistry',
    'modules',
    'requiredFiles',
    'roots',
    'sha256',
  ])
  assert.equal(profile.sha256, '6e598c22512a24659cbd0e533306abf3129733d67b1d120916b1b6b1581e29f2')
  assert.deepEqual(profile.modules, [
    { name: 'gimex.dll', base: 0x10000000 },
    { name: 'ijl10.dll', base: 0x10100000 },
  ])
  assert.equal(profile.requiredFiles.length, 15)
  assert(profile.roots.includes(0x665290))
  assert.equal(
    completeProfileJson(),
    profileText,
    'profile export is stable and contains no game bytes',
  )

  const compiler = new Compiler(executable())
  let artifact
  let metadata, bytes, plan, image
  try {
    compiler.addModule(executable(), 0x10000000)
    artifact = compiler.compile(Uint32Array.of(0x10001000))
    metadata = JSON.parse(artifact.runtimeMetadataJson())
    const full = JSON.parse(artifact.metadataJson())
    const keys = ['funcAddrs', 'funcCount', 'tableAddr', 'trampBase', 'entry', 'commandLineAddr']
    assert.deepEqual(metadata, Object.fromEntries(keys.map((key) => [key, full[key]])))
    assert(full.instructionMaps.length > 1, 'fixture includes omitted diagnostic maps')
    assert.deepEqual(metadata.funcAddrs, [0x401000, 0x10001000])
    bytes = artifact.wasm()
    assert.deepEqual(
      artifact.wasm(),
      bytes,
      'metadata export does not consume or alter module bytes',
    )
    plan = JSON.parse(compiler.loadPlanJson())
    image = compiler.loadBytes()
    assert.equal(plan.images.length, 2)
    assert(plan.spans.every((span) => span.offset + span.length <= image.length))
  } finally {
    artifact?.free()
    compiler.free()
  }

  // Instantiate after freeing the Rust owners, using only the lean metadata.
  const env = {
    memory: new WebAssembly.Memory({ initial: 256, maximum: 65536 }),
    table: new WebAssembly.Table({ element: 'anyfunc', initial: metadata.trampBase + 16384 }),
    dispatch() {
      assert.fail('unexpected guest API call')
    },
    blockTrace() {},
  }
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
  for (const name of names) env[name] = new WebAssembly.Global({ value: 'i32', mutable: true }, 0)
  new Uint32Array(env.memory.buffer).set(metadata.funcAddrs, metadata.tableAddr / 4)
  const { exports } = await WebAssembly.instantiate(await WebAssembly.compile(bytes), { env })
  for (const [index, address] of metadata.funcAddrs.entries()) {
    assert.equal(exports.idxOf(address), index + 1)
    env.esp.value = 0x10000
    exports.runAt(address)
    assert.equal(env.esp.value, 0x10004)
    assert.equal(env.actual_return.value, 0)
  }
  console.log(
    'Compiler profile and lean metadata: actual Wasm exports, module linking and ownership passed',
  )
}

function executable() {
  const bytes = new Uint8Array(0x400)
  const view = new DataView(bytes.buffer)
  const word = (at, value) => view.setUint16(at, value, true)
  const dword = (at, value) => view.setUint32(at, value, true)
  word(0, 0x5a4d)
  dword(0x3c, 0x80)
  dword(0x80, 0x4550)
  word(0x84, 0x14c)
  word(0x86, 1)
  word(0x94, 0xe0)
  word(0x98, 0x10b)
  dword(0xa8, 0x1000)
  dword(0xb4, 0x400000)
  dword(0xd0, 0x2000)
  dword(0xd4, 0x200)
  bytes.set(new TextEncoder().encode('.text'), 0x178)
  for (const [offset, value] of [
    [8, 0x200],
    [12, 0x1000],
    [16, 0x200],
    [20, 0x200],
    [36, 0x60000020],
  ])
    dword(0x178 + offset, value)
  bytes[0x200] = 0xc3
  return bytes
}
