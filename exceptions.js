import { registerNames } from './machine.js'

// Rust owns SEH records, chain traversal, calling convention and transfer decisions.
// The host executes requested handlers only after the Rust borrow has ended.
export async function dispatchException(dispatcher, id, args) {
  const { process, registers } = dispatcher
  const snapshot = () => Uint32Array.from(registerNames.map((name) => registers[name].value))
  const apply = (values) =>
    registerNames.forEach((name, i) => {
      registers[name].value = values[i]
    })
  let action = JSON.parse(process.beginException(id, args, snapshot()))

  while (action.kind === 'invoke') {
    const { token } = action
    apply(action.registers)

    try {
      await dispatcher.runRaw(action.address)
      dispatcher.selectThread()
      action = JSON.parse(process.resumeException(token, snapshot()))
    } catch (error) {
      try {
        dispatcher.selectThread()
        process.cancelException(token)
      } catch {
        // Preserve the handler/resume failure if the Rust owner is now poisoned.
      }

      throw error
    }
  }

  if (action.kind !== 'complete') throw new Error('Invalid exception continuation.')

  apply(action.registers)
}
