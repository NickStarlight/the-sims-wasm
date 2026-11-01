import { suspendWhenNeeded } from './conditional-suspension.js'
import { deferProcessTask } from './runtime-access.js'

// Each execution owner has one suspended guest stack; input is queued separately.
export class GuestThreadExit extends Error {
  constructor(code) {
    super(`Guest thread exited (${code}).`)
    this.code = code
  }
}

let taskChannel
const pendingTasks = []

// A message is an actual event-loop turn, without nested timer clamping.
export function yieldToHost(process) {
  const resume = (resolve) => () => deferProcessTask(process, resolve)

  // Node's MessagePort queue can keep its poll phase busy and delay timers.
  // setImmediate yields to its next check phase without the timer minimum.
  if (typeof globalThis.setImmediate === 'function')
    return new Promise((resolve) => globalThis.setImmediate(resume(resolve)))

  // Scheduler.yield continuations outrank worker messages and timers in Chrome.
  // A continuously running guest can starve input, frame acknowledgements and
  // sleeping guest threads even though it yields every slice. MessageChannel
  // puts the continuation on the ordinary task queue instead.
  if (!taskChannel) {
    taskChannel = new MessageChannel()

    taskChannel.port1.onmessage = () => {
      const resume = pendingTasks.shift()

      if (!pendingTasks.length) taskChannel.port1.unref?.()

      resume()
    }

    taskChannel.port2.unref?.()
    taskChannel.port1.unref?.()
  }

  return new Promise((resolve) => {
    pendingTasks.push(resume(resolve))
    taskChannel.port1.ref?.()
    taskChannel.port2.postMessage(null)
  })
}

export function createScheduler(signal, process) {
  return new Scheduler(signal, process)
}

class Scheduler {
  constructor(signal, process) {
    if (!WebAssembly.Suspending || !WebAssembly.promising) {
      throw new Error('Game execution requires Chrome 137+, Firefox 153+, or Safari 27+.')
    }

    Object.assign(this, { signal, process })
    this.sliceMs = 2
    this.deadline = performance.now() + this.sliceMs
    this.frameDeadline = 0
    this.pacedYields = 0
    this.pacedMs = 0
    this.yields = 0
    this.suspendedMs = 0
    this.longestYieldMs = 0
    this.checkpoint = suspendWhenNeeded(this.checkpoint)
  }

  checkpoint = () => {
    this.signal?.throwIfAborted()

    const started = performance.now()
    const frameDelay = this.frameDeadline - started

    if (started < this.deadline && frameDelay <= 0) return

    this.frameDeadline = 0
    const paced = frameDelay > 0
    this.yields++

    if (paced) this.pacedYields++

    const waiting = paced
      ? new Promise((resolve) =>
          setTimeout(() => deferProcessTask(this.process, resolve), Math.min(frameDelay, 16)),
        )
      : yieldToHost(this.process)

    return waiting.then(() => {
      const now = performance.now()
      const elapsed = now - started
      this.suspendedMs += elapsed

      if (paced) this.pacedMs += elapsed

      this.longestYieldMs = Math.max(this.longestYieldMs, elapsed)
      this.signal?.throwIfAborted()
      this.deadline = now + this.sliceMs
    })
  }

  deferFrame = (ms = 16) => {
    if (!Number.isFinite(ms) || ms < 0) throw new RangeError('Invalid frame delay.')

    this.frameDeadline = performance.now() + Math.min(ms, 16)
  }

  run = (fn, ...args) => {
    this.signal?.throwIfAborted()

    return deferProcessTask(this.process, () => {
      this.signal?.throwIfAborted()

      return WebAssembly.promising(fn)(...args)
    })
  }
}
