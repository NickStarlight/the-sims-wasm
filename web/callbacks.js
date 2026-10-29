import { GuestThreadExit } from './scheduler.js'

// Guest callbacks run after Rust has returned an explicit continuation request.
export function createGuestCaller(registers, names, memory, scheduler, exports) {
  return async (address, args) => {
    const saved = names.map((name) => registers[name].value)
    const savedControl = exports.fpuControl?.value
    const originalStack = registers.esp.value >>> 0
    const stack = originalStack - (args.length + 1) * 4
    const view = new DataView(memory.buffer)
    const stackWords = [0, ...args]
    stackWords.forEach((value, index) => view.setUint32(stack + index * 4, value, true))
    registers.esp.value = stack
    registers.resume_next.value = 0
    registers.actual_return.value = 0

    try {
      await scheduler.run(exports.runAt, address)
      const result = registers.eax.value >>> 0

      if (registers.esp.value >>> 0 !== originalStack) {
        throw new Error(
          `Guest callback 0x${address.toString(16)} returned with an unbalanced stack.`,
        )
      }

      return result
    } catch (error) {
      if (error instanceof GuestThreadExit) throw error

      throw new Error(
        `Guest callback 0x${address.toString(16)}: ${error.message}; instruction=0x${(registers.trap_va.value >>> 0).toString(16)}; target=0x${(registers.call_target.value >>> 0).toString(16)}`,
        { cause: error },
      )
    } finally {
      names.forEach((name, index) => {
        registers[name].value = saved[index]
      })

      if (savedControl !== undefined) exports.fpuControl.value = savedControl
    }
  }
}

export async function completeCall(process, request, callGuest, selectThread = () => {}) {
  let outcome = JSON.parse(request)

  while (outcome.kind === 'callback') {
    const { token } = outcome

    try {
      const result = await callGuest(outcome.address, outcome.args)
      selectThread()
      outcome = JSON.parse(process.resumeCall(token, result))
    } catch (error) {
      selectThread()
      process.cancelCall(token)
      throw error
    }
  }

  if (outcome.kind !== 'return') throw new Error('Invalid runtime call outcome.')

  return outcome.value
}
