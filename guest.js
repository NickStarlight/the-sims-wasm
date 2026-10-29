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
    mark('execution-module-ready')

    return module
  })
  // Setup may fail before awaiting compilation; avoid an unhandled rejection.
  compiling.catch(() => {})
  const machine = await createMachine(program, report)
  mark('assets-registered')
  const { process, memory, registers, assetCache, assetStats } = machine
  const presentFrame = presenter?.present ?? onPresent ?? ((owner) => consumeFrame(owner, onFrame))
  onReady(process)
  let storage
  let threads
  let progressTimer

  try {
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
    if (error instanceof GuestThreadExit) {
      report(error.message)

      return
    }

    const address = (name) => `0x${(registers[name].value >>> 0).toString(16)}`
    throw new Error(
      `${error.message}; guest instruction=${address('trap_va')}, target=${address('call_target')}, stack=${address('esp')}`,
      { cause: error },
    )
  } finally {
    clearInterval(progressTimer)
    presenter?.onTransfer?.(null)
    onReady(null)

    try {
      await threads?.close()
      await storage?.close()
    } finally {
      try {
        await assetCache?.close()
      } finally {
        process.free()
      }
    }
  }
}
