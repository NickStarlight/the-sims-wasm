import assert from 'node:assert/strict'
import { mkdtemp, readFile, rm, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { pathToFileURL } from 'node:url'

// Execute an actual Wasm trap, then simulate the poisoned binding's free error.
const trap = new WebAssembly.Instance(
  new WebAssembly.Module(
    Uint8Array.from([
      0, 97, 115, 109, 1, 0, 0, 0, 1, 4, 1, 96, 0, 0, 3, 2, 1, 0, 7, 7, 1, 3, 114, 117, 110, 0, 0,
      10, 5, 1, 3, 0, 0, 11,
    ]),
  ),
).exports.run

const fixture = `
let state
export function use(value) { state = value }
export default async function init() {}
export class Process { constructor() { return state.process } }
export const API_BASE = 0x800000
export function createFileReaders() { return () => () => new Uint8Array() }
export function protectProcess(process) { return process }
export function deferProcessTask(process, run) { return run() }
export async function openAssetCache() { return state.assetCache }
export async function createMachine() { return state.machine }
export function consumeFrame() {}
export class GuestThreadExit extends Error {}
export function initializationOrder(images) { return images }
export function prepareImports() { return {bindings: []} }
export async function openSaveStorage() {
  state.step('storage.open')
  return {close: async () => state.step('storage.close')}
}
export class ThreadHost {
  constructor() { this.waits = new Map(); this.tasks = new Set() }
  async close() { state.step('threads.close') }
}
export function createExecution() {
  return {
    instance: {exports: {run: () => state.step('run')}},
    scheduler: {run: fn => fn()},
    dispatcher: {},
    callGuest() {},
  }
}
`

export async function cleanupSmoke(directory) {
  const temp = await mkdtemp(join(tmpdir(), 'sims-runtime-cleanup-'))
  const previousReader = Object.getOwnPropertyDescriptor(globalThis, 'FileReaderSync')
  const originalInterval = globalThis.setInterval
  const originalClear = globalThis.clearInterval
  let current

  try {
    await writeFile(join(temp, 'fixture.mjs'), fixture)
    for (const name of ['machine', 'guest']) {
      const source = await readFile(new URL(`./${name}.js`, directory), 'utf8')
      await writeFile(
        join(temp, `${name}.mjs`),
        source.replace(/from '\.\/[^']+\.js'/g, "from './fixture.mjs'"),
      )
    }
    const { use, GuestThreadExit } = await import(pathToFileURL(join(temp, 'fixture.mjs')))
    const { createMachine } = await import(pathToFileURL(join(temp, 'machine.mjs')))
    const { describeGuestError, runGuest } = await import(pathToFileURL(join(temp, 'guest.mjs')))
    const native = new Error('recursive use of an object detected')
    native.stack = 'Error: recursive use\n    at Process.setThread (runtime.js:123)'
    const wrapped = new Error('guest instruction=0x42a3ae', { cause: native })
    wrapped.stack = 'Error: guest instruction=0x42a3ae\n    at runGuest (guest.js:100)'
    const report = describeGuestError(wrapped)
    assert.equal(report.error, wrapped.message, 'keep the launcher message unchanged')
    assert.equal(report.stack, wrapped.stack + '\nCaused by: ' + native.stack)
    assert.deepEqual(structuredClone(report), report, 'cause stacks survive worker messaging')
    native.cause = wrapped
    assert.deepEqual(describeGuestError(wrapped), report, 'cyclic causes terminate')
    assert.deepEqual(describeGuestError('plain failure'), {
      error: 'plain failure',
      stack: 'plain failure',
    })
    assert.deepEqual(describeGuestError({ message: 'Storage unavailable', stack: '' }), {
      error: 'Storage unavailable',
      stack: 'Storage unavailable',
    })
    Object.defineProperty(globalThis, 'FileReaderSync', {
      configurable: true,
      value: class {},
    })
    globalThis.setInterval = () => 99
    globalThis.clearInterval = () => current.step('timer.clear')

    const fresh = (primaryAt, cleanupFailures = new Map()) => {
      const state = {
        events: [],
        cleanupFailures,
        step(name) {
          this.events.push(name)
          if (name === primaryAt) {
            try {
              trap()
            } catch (error) {
              this.primary = error
              throw error
            }
          }
          if (this.cleanupFailures.has(name)) throw this.cleanupFailures.get(name)
        },
      }
      state.process = {
        reserve() {
          state.step('reserve')
        },
        adopt() {},
        write() {},
        setCommandLine() {},
        free() {
          state.step('process.free')
        },
      }
      state.assetCache = { close: async () => state.step('assets.close') }
      state.machine = {
        process: state.process,
        assetCache: state.assetCache,
        memory: new WebAssembly.Memory({ initial: 1 }),
        registers: Object.fromEntries(
          ['trap_va', 'call_target', 'esp'].map((name) => [name, { value: 0 }]),
        ),
      }
      current = state
      use(state)
      return state
    }
    const ownership = () =>
      new Error('attempted to take ownership of Rust value while it was borrowed')
    const wasm = Uint8Array.of(0, 97, 115, 109, 1, 0, 0, 0)
    const module = new WebAssembly.Module(wasm)
    const program = {
      module,
      metadata: {
        commandLineAddr: 0xd00000,
        tableAddr: 0xc00000,
        funcCount: 0,
        funcAddrs: [],
        trampBase: 0,
      },
      plan: { images: [], spans: [] },
      imageBytes: new Uint8Array(),
    }
    const invoke = (state, input = program) =>
      runGuest(input, () => {}, {
        onReady: (process) => state.step(process ? 'ready.set' : 'ready.clear'),
        onPhase: (phase) => state.step(`phase.${phase.name}`),
        presenter: {
          onTransfer: (callback) => state.step(callback ? 'presenter.set' : 'presenter.clear'),
        },
      })
    const releases = [
      'timer.clear',
      'presenter.clear',
      'ready.clear',
      'threads.close',
      'storage.close',
      'assets.close',
      'process.free',
    ]

    let state = fresh(
      'reserve',
      new Map([
        ['assets.close', new Error('asset close failed')],
        ['process.free', ownership()],
      ]),
    )
    await assert.rejects(
      createMachine(program, () => {}),
      (error) => error === state.primary && error instanceof WebAssembly.RuntimeError,
    )
    assert.deepEqual(
      state.events.slice(-2),
      ['assets.close', 'process.free'],
      'setup tries both releases after the original Wasm trap',
    )

    state = fresh('run', new Map(releases.map((name) => [name, ownership()])))
    await assert.rejects(
      invoke(state),
      (error) => error.cause === state.primary && /unreachable/.test(error.message),
    )
    assert.deepEqual(
      state.events.filter((name) => releases.includes(name)),
      releases,
      'every release is attempted despite failures',
    )

    for (const release of releases) {
      const cleanupError = new Error(`failed ${release}`)
      state = fresh(undefined, new Map([[release, cleanupError]]))
      await assert.rejects(invoke(state), (error) => error === cleanupError)
      assert.deepEqual(
        state.events.filter((name) => releases.includes(name)),
        releases,
        'cleanup failures surface after successful work without skipping later releases',
      )
    }

    for (const phase of ['phase.assets-registered', 'ready.set', 'storage.open']) {
      state = fresh(phase, new Map([['process.free', ownership()]]))
      await assert.rejects(
        invoke(state),
        (error) => error.cause === state.primary && /unreachable/.test(error.message),
      )
      assert.ok(state.events.includes('ready.clear'))
      assert.deepEqual(
        state.events.slice(-2),
        ['assets.close', 'process.free'],
        'early host failures still release the process and cache',
      )
    }

    state = fresh()
    await invoke(state)
    assert.deepEqual(
      state.events.filter((name) => releases.includes(name)),
      releases,
    )

    state = fresh()
    const transferred = { ...program, module: undefined, wasm }
    await invoke(state, transferred)
    assert.equal(transferred.wasm, undefined, 'release game source bytes after JIT compilation')
    assert.ok(
      state.events.includes('phase.execution-module-ready'),
      'the runtime can JIT transferred bytes without a prepared Module',
    )
    assert.deepEqual(
      state.events.filter((name) => releases.includes(name)),
      releases,
    )

    state = fresh(undefined, new Map([['process.free', ownership()]]))
    await assert.rejects(
      invoke(state, { ...program, module: undefined, wasm: Uint8Array.of(0) }),
      (error) => error.cause instanceof WebAssembly.CompileError,
      'JIT rejection of transferred bytes survives poisoned runtime cleanup',
    )
    assert.deepEqual(state.events.slice(-2), ['assets.close', 'process.free'])

    state = fresh(undefined, new Map([['run', new GuestThreadExit('Thread exited')]]))
    await invoke(state)
    assert.deepEqual(
      state.events.filter((name) => releases.includes(name)),
      releases,
    )
    const exitCleanup = ownership()
    state = fresh(
      undefined,
      new Map([
        ['run', new GuestThreadExit('Thread exited')],
        ['process.free', exitCleanup],
      ]),
    )
    await assert.rejects(invoke(state), (error) => error === exitCleanup)
    console.log(
      'Runtime cleanup: original Wasm traps retained, all releases attempted, cleanup-only failures surfaced',
    )
  } finally {
    globalThis.setInterval = originalInterval
    globalThis.clearInterval = originalClear
    if (previousReader) Object.defineProperty(globalThis, 'FileReaderSync', previousReader)
    else delete globalThis.FileReaderSync
    await rm(temp, { recursive: true, force: true })
  }
}
