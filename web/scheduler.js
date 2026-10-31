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
  if (!WebAssembly.Suspending || !WebAssembly.promising) {
    throw new Error('Game execution requires Chrome 137+, Firefox 153+, or Safari 27+.')
  }

  const sliceMs = 2
  let deadline = performance.now() + sliceMs
  let frameDeadline = 0
  let pacedYields = 0
  let pacedMs = 0
  let yields = 0
  let suspendedMs = 0
  let longestYieldMs = 0

  const checkpoint = () => {
    signal?.throwIfAborted()

    const started = performance.now()
    const frameDelay = frameDeadline - started

    if (started < deadline && frameDelay <= 0) return

    frameDeadline = 0
    const paced = frameDelay > 0
    yields++

    if (paced) pacedYields++

    const waiting = paced
      ? new Promise((resolve) =>
          setTimeout(() => deferProcessTask(process, resolve), Math.min(frameDelay, 16)),
        )
      : yieldToHost(process)

    return waiting.then(() => {
      const now = performance.now()
      const elapsed = now - started
      suspendedMs += elapsed

      if (paced) pacedMs += elapsed

      longestYieldMs = Math.max(longestYieldMs, elapsed)
      signal?.throwIfAborted()
      deadline = now + sliceMs
    })
  }

  return {
    // Yield the foreground at its next checkpoint while a child has CPU work.
    // The pause is bounded and does not depend on another paint acknowledgement.
    deferFrame(ms = 16) {
      if (!Number.isFinite(ms) || ms < 0) throw new RangeError('Invalid frame delay.')

      frameDeadline = performance.now() + Math.min(ms, 16)
    },
    get pacedYields() {
      return pacedYields
    },
    get pacedMs() {
      return pacedMs
    },
    checkpoint: suspendWhenNeeded(checkpoint),
    run: (fn, ...args) => {
      signal?.throwIfAborted()

      return deferProcessTask(process, () => {
        signal?.throwIfAborted()

        return WebAssembly.promising(fn)(...args)
      })
    },
    get yields() {
      return yields
    },
    get suspendedMs() {
      return suspendedMs
    },
    get longestYieldMs() {
      return longestYieldMs
    },
  }
}
