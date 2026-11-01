import { consumeFrame } from '../display/frame-output.js'
import { createMachine } from './machine.js'
import { GuestThreadExit } from './scheduler.js'
import { createExecution } from './execution.js'
import { ThreadHost } from './threads.js'
import { initializationOrder, prepareImports } from './linker.js'
import { openSaveStorage } from '../storage/index.js'
import { deferProcessTask } from './runtime-access.js'

// Preserve underlying runtime stacks when converting errors to worker messages.
// The guest-address wrapper alone cannot identify the failing native entry.
export function describeGuestError(error) {
  const stack = []
  const seen = new Set()

  for (let cause = error; cause != null && !seen.has(cause); cause = cause.cause) {
    seen.add(cause)
    stack.push(cause.stack || cause.message || String(cause))
  }

  return {
    error: error?.message ?? String(error),
    stack: stack.join('\nCaused by: '),
  }
}

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
    onDiagnostics = () => {},
  } = {},
) {
  const mark = (name) => onPhase({ name, atMs: performance.timeOrigin + performance.now() })
  const compiling = compileExecution(program, mark)
  const machine = await createMachine(program, report)
  const { process, registers, assetCache } = machine
  const state = { running: true }
  let failed = false

  try {
    const { instance, callGuest, scheduler, imports } = await prepareGuest(
      state,
      machine,
      program,
      compiling,
      report,
      mark,
      { onReady, onFrame, onRegistry, onPresent, presenter, onDiagnostics },
    )

    await initializeImages(program.plan.images, callGuest, report)

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
    state.running = false
    await releaseGuest(
      [
        () => clearInterval(state.progressTimer),
        () => onDiagnostics(null),
        () => presenter?.onTransfer?.(null),
        () => onReady(null),
        () => state.threads?.close(),
        () => state.storage?.close(),
        () => assetCache?.close(),
        () => process.free(),
      ],
      failed,
    )
  }
}

function compileExecution(program, mark) {
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

  return compiling
}

function observeExecution(
  machine,
  threads,
  execution,
  presenter,
  onDiagnostics,
  report,
  isRunning,
) {
  const { process, memory, registers, assetCache, assetStats } = machine
  const { scheduler, dispatcher } = execution
  onDiagnostics(() => ({
    atMs: performance.timeOrigin + performance.now(),
    calls: dispatcher.calls,
    lastCall: dispatcher.lastCall,
    yields: scheduler.yields,
    suspendedMs: scheduler.suspendedMs,
    pacedMs: scheduler.pacedMs,
    sourceReadMs: assetStats?.sourceReadMs ?? 0,
    sourceBytes: assetStats?.sourceBytes ?? 0,
    activeThreads: threads.tasks.size + 1,
    pendingWaits: threads.waits.size,
    guestMemoryBytes: memory.buffer.byteLength,
  }))
  presenter?.onTransfer?.(() => {
    // Leave time for background work without withholding the main renderer
    // for a whole 60 Hz frame whenever a child is runnable.
    if (threads.hasBackgroundWork()) scheduler.deferFrame(4)
  })

  return setInterval(() => {
    try {
      const pending = deferProcessTask(process, () => {
        if (!isRunning()) return

        assetCache?.flush()
        report(
          `Guest progress: ${dispatcher.calls} calls; last=${dispatcher.lastCall}; instruction=0x${(registers.trap_va.value >>> 0).toString(16)}; ${threads.waits.size} pending waits; ${threads.tasks.size} active threads; ${scheduler.yields} yields (${scheduler.suspendedMs.toFixed(1)} ms suspended, max ${scheduler.longestYieldMs.toFixed(1)} ms); paced=${scheduler.pacedYields}/${scheduler.pacedMs.toFixed(1)}ms; asset reads=${JSON.stringify({ ...assetStats, persistent: assetCache?.stats })}`,
        )
      })
      pending?.catch((error) => threads.abort(error))
    } catch (error) {
      threads.abort(error)
    }
  }, 5000)
}

async function initializeImages(images, callGuest, report) {
  for (const image of initializationOrder(images)) {
    if (!image.entry) continue

    report(`Initializing ${image.name ?? image.path}…`)

    if (!(await callGuest(image.entry, [image.base, 1, 0]))) {
      throw new Error(`DLL initialization failed: ${image.name}`)
    }
  }
}

async function releaseGuest(resources, failed) {
  let cleanupFailed = false
  let cleanupError

  // Keep the first failure and attempt every release. In particular, freeing
  // a binding poisoned by a raw trap must not hide the original exception.
  for (const close of resources) {
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

async function prepareGuest(state, machine, program, compiling, report, mark, hooks) {
  const { process, memory } = machine
  const { onReady, onFrame, onRegistry, onPresent, presenter, onDiagnostics } = hooks
  mark('assets-registered')
  const presentFrame = presenter?.present ?? onPresent ?? ((owner) => consumeFrame(owner, onFrame))
  onReady(process)
  state.storage = await openSaveStorage(program.files ?? [], process, program.clientId)
  mark('saves-restored')
  const module = await compiling
  const imports = prepareImports(process, program.plan, program.metadata, memory)
  state.threads = new ThreadHost(
    process,
    memory,
    module,
    program.metadata,
    imports,
    state.storage,
    onRegistry,
    report,
    presentFrame,
  )
  const { instance, callGuest, scheduler, dispatcher } = createExecution(
    module,
    machine,
    imports,
    state.threads,
    report,
    presentFrame,
  )
  state.progressTimer = observeExecution(
    machine,
    state.threads,
    { scheduler, dispatcher },
    presenter,
    onDiagnostics,
    report,
    () => state.running,
  )

  return { instance, callGuest, scheduler, imports }
}
