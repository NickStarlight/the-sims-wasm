import { consumeFrame } from './frame-output.js'
import { createMachine } from './machine.js'
import { GuestThreadExit } from './scheduler.js'
import { createExecution } from './execution.js'
import { ThreadHost } from './threads.js'
import { initializationOrder, prepareImports } from './linker.js'
import { openSaveStorage } from './storage.js'

export async function runGuest(
  program,
  report,
  {
    onReady = () => {},
    onFrame = () => {},
    onRegistry = () => {},
    onPresent,
    presenter,
    onPhase = () => {},
  } = {},
) {
  const mark = (name) => onPhase({ name, atMs: performance.timeOrigin + performance.now() })
  // Compile independently of asset registration and save-journal restoration.
  const compiling = (
    program.module ? Promise.resolve(program.module) : WebAssembly.compile(program.wasm)
  ).then((module) => {
    // Execution needs only the compiled module. Drop the transferred source
    // buffer instead of retaining another complete copy for the whole game.
    if (!program.module) program.wasm = undefined

    mark('execution-module-ready')

    return module
  })
  // Setup may fail before awaiting compilation; avoid an unhandled rejection.
  compiling.catch(() => {})
  const machine = await createMachine(program, report)
  const { process, memory, registers, assetCache, assetStats } = machine
  let storage
  let threads
  let progressTimer
  let failed = false

  try {
    mark('assets-registered')
    const presentFrame =
      presenter?.present ?? onPresent ?? ((owner) => consumeFrame(owner, onFrame))
    onReady(process)
    storage = await openSaveStorage(program.files ?? [], process, program.clientId)
    mark('saves-restored')
    const module = await compiling
    const imports = prepareImports(process, program.plan, program.metadata, memory)
    threads = new ThreadHost(
      process,
      memory,
      module,
      program.metadata,
      imports,
      storage,
      onRegistry,
      report,
      presentFrame,
    )
    const { instance, callGuest, scheduler, dispatcher } = createExecution(
      module,
      machine,
      imports,
      threads,
      report,
      presentFrame,
    )
    presenter?.onTransfer?.(() => {
      if (threads.hasBackgroundWork()) scheduler.deferFrame(16)
    })
    progressTimer = setInterval(() => {
      assetCache?.flush()
      report(
        `Guest progress: ${dispatcher.calls} calls; last=${dispatcher.lastCall}; instruction=0x${(registers.trap_va.value >>> 0).toString(16)}; ${threads.waits.size} pending waits; ${threads.tasks.size} active threads; ${scheduler.yields} yields (${scheduler.suspendedMs.toFixed(1)} ms suspended, max ${scheduler.longestYieldMs.toFixed(1)} ms); paced=${scheduler.pacedYields}/${scheduler.pacedMs.toFixed(1)}ms; asset reads=${JSON.stringify({ ...assetStats, persistent: assetCache?.stats })}`,
      )
    }, 5000)

    for (const image of initializationOrder(program.plan.images)) {
      if (!image.entry) continue

      report(`Initializing ${image.name ?? image.path}…`)

      if (!(await callGuest(image.entry, [image.base, 1, 0]))) {
        throw new Error(`DLL initialization failed: ${image.name}`)
      }
    }

    report(`Running ${program.metadata.funcCount} functions; ${imports.bindings.length} imports`)
    mark('guest-entry')
    await scheduler.run(instance.exports.run, program.metadata.entry)
    report('Guest entry returned.')
  } catch (error) {
    failed = true

    if (error instanceof GuestThreadExit) {
      report(error.message)
      failed = false

      return
    }

    const address = (name) => `0x${(registers[name].value >>> 0).toString(16)}`
    throw new Error(
      `${error.message}; guest instruction=${address('trap_va')}, target=${address('call_target')}, stack=${address('esp')}`,
      { cause: error },
    )
  } finally {
    let cleanupFailed = false
    let cleanupError

    // Keep the first failure and attempt every release. In particular, freeing
    // a binding poisoned by a raw trap must not hide the original exception.
    for (const close of [
      () => clearInterval(progressTimer),
      () => presenter?.onTransfer?.(null),
      () => onReady(null),
      () => threads?.close(),
      () => storage?.close(),
      () => assetCache?.close(),
      () => process.free(),
    ]) {
      try {
        const pending = close()

        if (pending && typeof pending.then === 'function') await pending
      } catch (error) {
        if (!cleanupFailed) {
          cleanupFailed = true
          cleanupError = error
        }
      }
    }

    if (!failed && cleanupFailed) throw cleanupError
  }
}
