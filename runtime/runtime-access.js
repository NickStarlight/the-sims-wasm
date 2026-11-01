// Safari can dispatch worker events during FileReaderSync reads. A reader is
// called from inside a Rust Process export, whose mutable borrow is still live.
// Keep the gate around the entire export, including its return into JavaScript.
const access = new WeakMap()

export function protectProcess(process) {
  if (access.has(process)) return process

  const state = { depth: 0, pending: [], timer: null }
  access.set(process, state)

  for (const name of Object.getOwnPropertyNames(Object.getPrototypeOf(process))) {
    if (name === 'constructor' || name.startsWith('__')) continue

    const method = process[name]

    if (typeof method !== 'function') continue

    process[name] = function (...args) {
      state.depth++

      try {
        return method.apply(this, args)
      } finally {
        state.depth--
        schedule(state)
      }
    }
  }

  return process
}

function schedule(state) {
  if (state.depth || state.timer !== null || !state.pending.length) return

  // Never flush from the export's finally block: Rust and the guest's import
  // postprocessing must both unwind first. One task per turn also lets resumed
  // JSPI stacks settle before another deferred event can enter a file reader.
  state.timer = setTimeout(() => {
    state.timer = null

    if (state.depth) return

    const { run, resolve, reject } = state.pending.shift()

    try {
      resolve(run())
    } catch (error) {
      reject(error)
    } finally {
      schedule(state)
    }
  }, 0)
}

// Idle calls retain their synchronous result/exception. Deferred calls return a
// Promise so each event owner can handle errors and await shutdown normally.
export function deferProcessTask(process, run) {
  const state = access.get(process)

  if (!state || (!state.depth && !state.pending.length)) return run()

  return new Promise((resolve, reject) => {
    state.pending.push({ run, resolve, reject })
    schedule(state)
  })
}
