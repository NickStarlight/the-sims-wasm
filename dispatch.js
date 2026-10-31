import { dispatchException } from './exceptions.js'
import { completeCall } from './callbacks.js'
import { registerNames } from './machine.js'
import { GuestThreadExit } from './scheduler.js'

export class ApiDispatcher {
  constructor(machine, scheduler, report, presentFrame) {
    Object.assign(this, machine)
    this.scheduler = scheduler
    this.report = report
    this.presentFrame = presentFrame
    this.calls = 0
    this.bindings = []
    this.argumentArrays = []
    this.smallCalls = typeof this.process.callSmall === 'function'
    this.threadedSmallCalls = typeof this.process.callSmallForThread === 'function'
    this.scalarStatusCalls =
      typeof this.process.callSmallForThreadStatus === 'function' &&
      typeof this.process.takeSyncCallError === 'function'
    this.stackView = null
    this.callGuest = null
    this.import = new WebAssembly.Suspending((slot) => this.dispatch(slot))
    this.pendingSchedule = null
  }

  dispatch(slot) {
    this.checkAbort()

    if (slot > 0 && slot <= 10) {
      this.selectThread()
      const registers = Int32Array.from(registerNames.map((name) => this.registers[name].value))
      const result = this.process.cpuHelper(slot, registers, performance.now())
      registerNames.forEach((name, index) => {
        this.registers[name].value = result[index]
      })

      return
    }

    const binding = this.bindings[slot - 16]

    if (!binding) {
      const target = (this.registers.call_target.value >>> 0).toString(16)
      const instruction = (this.registers.trap_va.value >>> 0).toString(16)
      const snapshot = registerNames
        .slice(0, 8)
        .map((name) => `${name}=0x${(this.registers[name].value >>> 0).toString(16)}`)
        .join(' ')
      throw new Error(`Unknown guest call 0x${target} at instruction 0x${instruction}; ${snapshot}`)
    }

    if (!binding.api) {
      const stack = this.registers.esp.value >>> 0
      const view = new DataView(this.memory.buffer)
      const words = Array.from(
        { length: 5 },
        (_, index) => '0x' + view.getUint32(stack + index * 4, true).toString(16),
      )
      throw new Error(`Unimplemented import: ${binding.label}; return/stack=${words.join(',')}`)
    }

    return this.callImport(binding)
  }

  dispatchSync(slot) {
    const binding = this.bindings[slot - 16]
    const api = binding?.api

    if (api && (api.schedule || api.callback || api.exception || api.media))
      throw new Error('This API requires suspending dispatch.')

    if (api && this.threadedSmallCalls && !api.variadic && api.arity <= 4) {
      this.checkAbort()

      return this.callSynchronousScalar(binding)
    }

    const result = this.dispatch(slot)

    if (result && typeof result.then === 'function')
      throw new Error('Synchronous dispatch returned a pending operation.')

    return result
  }

  checkAbort() {
    this.threads.signal.throwIfAborted()
  }

  // Ordinary small APIs need neither retained argument arrays nor continuation
  // closures. Rust selects their owner within the same call, even if another
  // guest thread or an external host call changed the current thread.
  callSynchronousScalar(binding) {
    const api = binding.api
    this.lastCall = binding.label
    const stack = this.registers.esp.value >>> 0
    let first = 0
    let second = 0
    let third = 0
    let fourth = 0
    let returnAddress = 0
    let entered = false

    try {
      const buffer = this.memory.buffer

      if (this.stackView?.buffer !== buffer) this.stackView = new DataView(buffer)

      const view = this.stackView
      returnAddress = view.getUint32(stack, true)

      if (api.arity > 0) first = view.getUint32(stack + 4, true)

      if (api.arity > 1) second = view.getUint32(stack + 8, true)

      if (api.arity > 2) third = view.getUint32(stack + 12, true)

      if (api.arity > 3) fourth = view.getUint32(stack + 16, true)

      const wall = api.wallClock
        ? Date.now() - (api.wallClock === 'local' ? new Date().getTimezoneOffset() * 60000 : 0)
        : undefined
      const now = api.clock ? performance.now() : undefined
      entered = true
      let value

      if (this.scalarStatusCalls) {
        value = this.process.callSmallForThreadStatus(
          this.threadId,
          api.id,
          api.arity,
          first,
          second,
          third,
          fourth,
          now,
          wall,
        )

        if (value < 0) throw this.process.takeSyncCallError()
      } else {
        value = this.process.callSmallForThread(
          this.threadId,
          api.id,
          api.arity,
          first,
          second,
          third,
          fourth,
          now,
          wall,
        )
      }

      this.finish(binding, stack, returnAddress, value, false)
    } catch (error) {
      if (!entered) {
        // Keep the previous thread-selection behavior even for an invalid stack.
        this.selectThread()

        throw error
      }

      if (error instanceof GuestThreadExit) throw error

      const args = [first, second, third, fourth].slice(0, api.arity)

      throw new Error(
        `${binding.label}: ${error.message}; args=${args.map((value) => '0x' + value.toString(16)).join(',')} (caller 0x${returnAddress.toString(16)})`,
        { cause: error },
      )
    }
  }

  dispatchSchedule(slot) {
    const api = this.bindings[slot - 16]?.api

    if (api && (!api.schedule || api.callback || api.exception || api.media))
      throw new Error('This API requires a different dispatch path.')

    if (this.pendingSchedule) throw new Error('A scheduled operation is already pending.')

    const result = this.dispatch(slot)

    if (result && typeof result.then === 'function') {
      this.pendingSchedule = result

      return 1
    }

    return 0
  }

  awaitSchedule() {
    const pending = this.pendingSchedule

    if (!pending) throw new Error('No scheduled operation is pending.')

    this.pendingSchedule = null

    return pending
  }

  callImport(binding, timersPumped = false) {
    this.lastCall = binding.label

    if (binding.api.media && !timersPumped)
      return this.pumpMediaTimers().then(() => this.callImport(binding, true))

    try {
      this.selectThread()
    } catch (error) {
      throw new Error(`${binding.label}: ${error.message} (selecting thread ${this.threadId})`, {
        cause: error,
      })
    }

    const stack = this.registers.esp.value >>> 0
    const buffer = this.memory.buffer

    if (this.stackView?.buffer !== buffer) this.stackView = new DataView(buffer)

    const view = this.stackView
    const { arity, variadic } = binding.api
    const length = arity + (variadic ? 1 : 0)
    // Calls that can suspend or reenter must retain independent arguments.
    const independent = binding.api.exception || binding.api.schedule || binding.api.callback
    const args = independent
      ? new Uint32Array(length)
      : (this.argumentArrays[length] ??= new Uint32Array(length))

    for (let index = 0; index < arity; index++) {
      args[index] = view.getUint32(stack + 4 + index * 4, true)
    }

    // Rust reads only the variadic words required by the guest format string.
    if (variadic) args[arity] = stack + 4 + arity * 4

    const returnAddress = view.getUint32(stack, true)
    const finish = (value) => this.finish(binding, stack, returnAddress, value)
    const failure = (error) =>
      error instanceof GuestThreadExit
        ? error
        : new Error(
            `${binding.label}: ${error.message}; args=${[...args]
              .map((value) => '0x' + value.toString(16))
              .join(',')} (caller 0x${returnAddress.toString(16)})`,
            { cause: error },
          )

    try {
      if (binding.api.exception) return dispatchException(this, binding.api.id, args)

      if (binding.api.schedule) {
        const result = this.threads.schedule(binding.api.id, args)

        if (result instanceof Promise)
          return result.then(finish, (error) => {
            throw failure(error)
          })

        // An immediate schedule result has not yielded or entered another thread.
        return this.finish(binding, stack, returnAddress, result, false)
      }

      if (binding.api.callback) {
        return completeCall(
          this.process,
          this.process.beginCall(binding.api.id, args),
          this.callGuest,
          () => this.selectThread(),
        ).then(finish, (error) => {
          throw failure(error)
        })
      }

      const wall = binding.api.wallClock
        ? Date.now() -
          (binding.api.wallClock === 'local' ? new Date().getTimezoneOffset() * 60000 : 0)
        : undefined
      const now = binding.api.clock ? performance.now() : undefined
      const value =
        this.smallCalls && length <= 4
          ? this.process.callSmall(
              binding.api.id,
              length,
              args[0] ?? 0,
              args[1] ?? 0,
              args[2] ?? 0,
              args[3] ?? 0,
              now,
              wall,
            )
          : this.process.call(binding.api.id, args, now, wall)
      // A synchronous runtime call cannot switch the selected guest thread.
      // Suspended and callback completions still select it again in finish().
      this.finish(binding, stack, returnAddress, value, false)
    } catch (error) {
      throw failure(error)
    }
  }

  async pumpMediaTimers() {
    // Pump WinMM timers with a fixed sample; advance before the callback,
    // then re-query so cancellation/reentry can change the remaining timers.
    const now = performance.now()

    for (;;) {
      this.selectThread()
      const record = this.process.nextMediaTimer(now)

      if (!record) return

      const callback = JSON.parse(record)
      await this.callGuest(callback.address, callback.args)
    }
  }

  selectThread() {
    this.process.setThread(this.threadId)
  }

  present() {
    this.presentFrame(this.process)
  }

  finish(binding, stack, returnAddress, value, selectThread = true) {
    if (selectThread) this.selectThread()

    if (binding.api.storage) this.storage.flush()

    if (binding.api.dll === 'advapi32.dll') {
      const registry = this.process.takeRegistryChanges()

      if (registry !== undefined) this.onRegistry(registry)
    }

    this.registers.eax.value = value
    this.registers.actual_return.value = returnAddress
    this.registers.esp.value = stack + 4 + binding.api.stackArgs * 4

    if (binding.api.dll === 'ddraw.dll') this.present()

    if (binding.api.wake) this.threads.notify(binding.api.name)

    if (++this.calls <= 40 || this.calls % 100000 === 0) {
      this.report(`${this.calls} calls: ${binding.label}; ${this.scheduler.yields} yields`)
    }
  }
}
