import { createThreadContext } from './machine.js'
import { createExecution } from './execution.js'
import { GuestThreadExit } from './scheduler.js'

// Rust owns lifecycle, wait decisions and register descriptors. JS owns only suspended stacks.
export class ThreadHost {
  constructor(
    process,
    memory,
    module,
    metadata,
    imports,
    storage,
    onRegistry,
    report,
    presentFrame,
  ) {
    Object.assign(this, {
      process,
      memory,
      module,
      metadata,
      imports,
      storage,
      onRegistry,
      report,
      presentFrame,
    })
    this.smallSchedule = typeof process.scheduleCallSmall === 'function'
    this.readyQuery = typeof process.hasReadyWaits === 'function'
    this.cancellation = new AbortController()
    this.signal = this.cancellation.signal
    this.abortGlobal = new WebAssembly.Global({ value: 'i32', mutable: true }, 0)
    this.aborted = false
    // Abort dispatch is synchronous. Mirror its state once instead of entering
    // a DOM method for each of the guest's synchronous API calls.
    this.signal.addEventListener(
      'abort',
      () => {
        // This listener is installed before the host escapes its constructor.
        // Native trampolines must see cancellation even during abort reentry.
        this.abortGlobal.value = 1
        this.aborted = true
      },
      { once: true },
    )
    this.waits = new Map()
    this.tasks = new Set()
    this.timer = null
    this.timerDeadline = null
    this.launchScheduled = false
  }

  checkAbort() {
    if (this.aborted) this.signal.throwIfAborted()
  }

  schedule(id, args) {
    const now = performance.now()
    const result =
      this.smallSchedule && args.length <= 4
        ? this.process.scheduleCallSmall(
            id,
            args.length,
            args[0] ?? 0,
            args[1] ?? 0,
            args[2] ?? 0,
            args[3] ?? 0,
            now,
          )
        : this.process.scheduleCall(id, args, now)

    if (typeof result === 'number') return result

    const outcome = JSON.parse(result)

    if (outcome.kind === 'exit') throw new GuestThreadExit(outcome.code)

    if (outcome.kind === 'return') return outcome.value

    if (outcome.kind !== 'wait') throw new Error('Invalid scheduled call outcome.')

    return new Promise((resolve, reject) => {
      this.waits.set(outcome.token, { resolve, reject, deadline: outcome.deadlineMs })
      this.drain(false)
    })
  }

  hasBackgroundWork() {
    // The main stack is not in tasks, while waits may include it. This is a
    // conservative check: at least one child must be outside a native wait.
    return this.tasks.size > this.waits.size
  }

  notify(name) {
    if (name === 'CreateThread' || name === 'ResumeThread') this.drain()
    else if (this.waits.size && (!this.readyQuery || this.process.hasReadyWaits()))
      this.drain(false)
  }

  drain(launch = true) {
    if (this.signal.aborted) return

    this.process.advanceTime(performance.now())

    for (const ready of JSON.parse(this.process.takeReadyWaits())) {
      const pending = this.waits.get(ready.token)

      if (!pending) throw new Error('Completed guest wait has no host continuation.')

      this.waits.delete(ready.token)
      pending.resolve(ready.value)
    }

    if (launch && !this.launchScheduled) {
      this.launchScheduled = true
      queueMicrotask(() => {
        this.launchScheduled = false

        if (this.signal.aborted) return

        try {
          for (const descriptor of JSON.parse(this.process.takeRunnableThreads()))
            this.start(descriptor)
        } catch (error) {
          this.abort(error)
        }
      })
    }

    const deadlines = [...this.waits.values()]
      .map((wait) => wait.deadline)
      .filter((value) => value !== null)
    const deadline = deadlines.length ? Math.min(...deadlines) : null

    if (this.timer !== null && deadline === this.timerDeadline) return

    clearTimeout(this.timer)
    this.timer = null
    this.timerDeadline = deadline

    if (deadline !== null) {
      const delay = Math.max(0, Math.min(0x7fffffff, deadline - performance.now()))
      this.timer = setTimeout(() => {
        this.timer = null
        this.timerDeadline = null

        try {
          this.drain()
        } catch (error) {
          this.abort(error)
        }
      }, delay)
    }
  }

  start(descriptor) {
    // Rust has committed the runnable transition before returning this descriptor.
    const task = Promise.resolve()
      .then(async () => {
        this.signal.throwIfAborted()
        const machine = {
          process: this.process,
          memory: this.memory,
          ...createThreadContext(this.metadata, descriptor),
        }
        const execution = createExecution(
          this.module,
          machine,
          this.imports,
          this,
          this.report,
          this.presentFrame,
        )
        const code = await execution
          .callGuest(descriptor.entry, [descriptor.parameter])
          .catch((error) => {
            if (!(error instanceof GuestThreadExit)) throw error

            return error.code
          })

        this.process.finishThread(descriptor.id, code)
        this.drain()
      })
      .catch((error) => {
        this.process.finishThread(descriptor.id, 0xc0000005)
        this.abort(error)
      })
    this.tasks.add(task)
    task.finally(() => this.tasks.delete(task))
  }

  abort(reason) {
    if (this.signal.aborted) return

    this.cancellation.abort(reason)
    clearTimeout(this.timer)

    for (const [token, wait] of this.waits) {
      this.process.cancelWait(token)
      wait.reject(reason)
    }

    this.waits.clear()
  }

  async close() {
    this.abort(new Error('Guest process stopped.'))
    await Promise.allSettled([...this.tasks])
  }
}
