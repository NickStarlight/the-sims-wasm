import { ApiDispatcher } from './dispatch.js'
import { createScheduler } from './scheduler.js'
import { createGuestCaller } from './callbacks.js'
import { attachImports } from './linker.js'
import { registerNames } from './machine.js'
import { installSynchronousImports, installScheduledImports } from './sync-imports.js'

export function createExecution(module, machine, imports, host, report, presentFrame) {
  const scheduler = createScheduler(host.signal)
  const dispatcher = new ApiDispatcher(machine, scheduler, report, presentFrame)
  dispatcher.threads = host

  if (host.checkAbort) dispatcher.checkAbort = host.checkAbort.bind(host)

  dispatcher.storage = host.storage
  dispatcher.onRegistry = host.onRegistry
  const { memory, table, registers } = machine
  const instance = new WebAssembly.Instance(module, {
    env: {
      memory,
      table,
      ...registers,
      dispatch: dispatcher.import,
      blockTrace: scheduler.checkpoint,
    },
  })
  dispatcher.runRaw = (address) => scheduler.run(instance.exports.runAt, address)
  dispatcher.bindings = imports.bindings
  installSynchronousImports(table, imports, (slot) => dispatcher.dispatchSync(slot), dispatcher)
  installScheduledImports(
    table,
    imports,
    (slot) => dispatcher.dispatchSchedule(slot),
    () => dispatcher.awaitSchedule(),
  )
  attachImports(table, imports)
  const callGuest = createGuestCaller(registers, registerNames, memory, scheduler, instance.exports)
  dispatcher.callGuest = callGuest

  return { scheduler, callGuest, instance, dispatcher }
}
