import { GuestThreadExit } from './scheduler.js'

const modules = new WeakMap()
const abortGlobals = new WeakMap()
const encoder = new TextEncoder()
const rectangleCalls = new Set([
  'OffsetRect',
  'InflateRect',
  'IntersectRect',
  'UnionRect',
  'IsRectEmpty',
  'PtInRect',
])

function eligible(binding) {
  const api = binding?.api
  const dll = api?.dll?.toLowerCase()
  // Statically resolved API records omit the export name; the linker retains it
  // in the binding label for both static and dynamically resolved imports.
  const label = binding?.label
  const name = typeof label === 'string' ? label.slice(label.lastIndexOf('!') + 1) : ''

  return (
    (dll === 'kernel32.dll' || (dll === 'user32.dll' && rectangleCalls.has(name))) &&
    Number.isInteger(api.arity) &&
    api.arity >= 0 &&
    api.arity <= 4 &&
    (api.stackArgs === 0 || api.stackArgs === api.arity) &&
    !api.variadic &&
    !api.schedule &&
    !api.wake &&
    !api.storage &&
    !api.media &&
    !api.callback &&
    !api.exception
  )
}

function abortState(signal) {
  let value = abortGlobals.get(signal)

  if (!value) {
    value = new WebAssembly.Global({ value: 'i32', mutable: true }, signal.aborted ? 1 : 0)

    // One listener per host signal. Its closure holds only the shared global,
    // never a thread's dispatcher, instance or memory. Closed hosts stay aborted.
    if (!signal.aborted)
      signal.addEventListener(
        'abort',
        () => {
          value.value = 1
        },
        { once: true },
      )

    abortGlobals.set(signal, value)
  }

  return value
}

function failure(binding, address, first, second, third, fourth, error) {
  if (error instanceof GuestThreadExit) return error

  const args = [first, second, third, fourth].slice(0, binding.api.arity)

  return new Error(
    `${binding.label}: ${error.message}; args=${args.map((value) => '0x' + (value >>> 0).toString(16)).join(',')} (caller 0x${(address >>> 0).toString(16)})`,
    { cause: error },
  )
}

// Replace pure scalar kernel calls and the explicit rectangle services, whose
// finish() path has no host side effects beyond registers and progress reports.
// Other imports retain their callbacks, waits, media and completion side effects.
export function installNativeImports(table, imports, host) {
  const signal = host?.threads?.signal

  if (
    typeof host?.process?.callSmallForThreadStatus !== 'function' ||
    typeof host?.process?.takeSyncCallError !== 'function' ||
    typeof signal?.addEventListener !== 'function' ||
    typeof signal?.throwIfAborted !== 'function'
  )
    return 0

  const { bindings, trap } = imports

  if (!Array.isArray(bindings)) throw new TypeError('Import bindings must be an array.')

  if (!Number.isSafeInteger(trap) || trap < 0)
    throw new RangeError('Invalid import trampoline base.')

  const entries = bindings.flatMap((binding, index) =>
    eligible(binding)
      ? [{ slot: index + 16, arity: binding.api.arity, stackArgs: binding.api.stackArgs }]
      : [],
  )

  if (entries.length && trap + entries.at(-1).slot >= table.length)
    throw new RangeError('Native import trampolines exceed the function table.')

  if (!entries.length) return 0

  const shared = Object.prototype.toString.call(host.memory.buffer) === '[object SharedArrayBuffer]'
  let cached = modules.get(bindings)

  if (
    !cached ||
    cached.shared !== shared ||
    cached.entries.length !== entries.length ||
    entries.some((entry, i) => {
      const old = cached.entries[i]

      return (
        old.slot !== entry.slot || old.arity !== entry.arity || old.stackArgs !== entry.stackArgs
      )
    })
  ) {
    cached = { entries, shared, module: compile(entries, shared) }
    modules.set(bindings, cached)
  }

  const reportDue = new WebAssembly.Global({ value: 'i32', mutable: true }, 0)
  const ownedAbort = host.threads.abortGlobal
  let pendingReport

  const invoke = (slot, address, first, second, third, fourth) => {
    // ThreadHost sets its owned global in its first abort listener. Arbitrary
    // hosts can reenter from an earlier listener, before our fallback mirror.
    if (!ownedAbort) signal.throwIfAborted()

    const binding = bindings[slot - 16]
    const api = binding.api
    host.lastCall = binding.label

    try {
      const wall = api.wallClock
        ? Date.now() - (api.wallClock === 'local' ? new Date().getTimezoneOffset() * 60000 : 0)
        : undefined
      const now = api.clock ? performance.now() : undefined
      const value = host.process.callSmallForThreadStatus(
        host.threadId,
        api.id,
        api.arity,
        first,
        second,
        third,
        fourth,
        now,
        wall,
      )

      if (value < 0) throw host.process.takeSyncCallError()

      if (++host.calls <= 40 || host.calls % 100000 === 0) {
        pendingReport = { binding, address, first, second, third, fourth }
        reportDue.value = 1
      }

      return value
    } catch (error) {
      throw failure(binding, address, first, second, third, fourth, error)
    }
  }

  const report = () => {
    reportDue.value = 0
    const state = pendingReport
    pendingReport = undefined

    try {
      host.report(`${host.calls} calls: ${state.binding.label}; ${host.scheduler.yields} yields`)
    } catch (error) {
      throw failure(
        state.binding,
        state.address,
        state.first,
        state.second,
        state.third,
        state.fourth,
        error,
      )
    }
  }

  const instance = new WebAssembly.Instance(cached.module, {
    env: {
      invoke,
      fallback: (slot) => {
        signal.throwIfAborted()

        return host.dispatchSync(slot)
      },
      report,
      memory: host.memory,
      ...host.registers,
      aborted: ownedAbort ?? abortState(signal),
      reportDue,
    },
  })

  for (const { slot } of entries) table.set(trap + slot, instance.exports[`f${slot}`])

  return entries.length
}

function unsigned(value) {
  const bytes = []

  do {
    const byte = value & 127
    value >>>= 7
    bytes.push(byte | (value ? 128 : 0))
  } while (value)

  return bytes
}

function signed(value) {
  const bytes = []
  value |= 0

  for (;;) {
    const byte = value & 127
    value >>= 7
    const done = (value === 0 && !(byte & 64)) || (value === -1 && byte & 64)
    bytes.push(byte | (done ? 0 : 128))

    if (done) return bytes
  }
}

function name(value) {
  const bytes = encoder.encode(value)

  return [...unsigned(bytes.length), ...bytes]
}

function constant(value) {
  return [0x41, ...signed(value)]
}

function compile(entries, shared) {
  const bytes = [0, 97, 115, 109, 1, 0, 0, 0]

  const section = (id, payload) => {
    bytes.push(id, ...unsigned(payload.length))

    for (const byte of payload) bytes.push(byte)
  }

  // invoke(slot, snapshotted return address, four arguments) -> DWORD;
  // fallback(slot) -> void; report() -> void. Wrappers remain () -> void.
  section(1, [3, 0x60, 6, ...Array(6).fill(0x7f), 1, 0x7f, 0x60, 1, 0x7f, 0, 0x60, 0, 0])
  const imports = [
    9,
    ...name('env'),
    ...name('invoke'),
    0,
    0,
    ...name('env'),
    ...name('fallback'),
    0,
    1,
    ...name('env'),
    ...name('report'),
    0,
    2,
    ...name('env'),
    ...name('memory'),
    2,
    ...(shared ? [3, 0, ...unsigned(65536)] : [0, 0]),
  ]

  for (const global of ['esp', 'eax', 'actual_return', 'aborted', 'reportDue'])
    imports.push(...name('env'), ...name(global), 3, 0x7f, 1)

  section(2, imports)
  section(3, [...unsigned(entries.length), ...entries.map(() => 2)])
  const exports = unsigned(entries.length)
  const bodies = unsigned(entries.length)

  for (const [index, { slot, arity, stackArgs }] of entries.entries()) {
    exports.push(...name(`f${slot}`), 0, ...unsigned(index + 3))
    const fallback = [...constant(slot), 0x10, 1, 0x0f]
    const body = [
      1,
      2,
      0x7f,
      // Abort before inspecting memory, preserving the exact host cancellation.
      0x23,
      3,
      0x04,
      0x40,
      ...fallback,
      0x0b,
      // Unsigned i64 bounds admit an end exactly at 4GiB without wrapping.
      // Invalid stacks use the existing JS error/thread-selection path.
      0x23,
      0,
      0x22,
      0,
      0xad,
      0x42,
      ...signed((1 + arity) * 4),
      0x7c,
      0x3f,
      0,
      0xad,
      0x42,
      16,
      0x86,
      0x56,
      0x04,
      0x40,
      ...fallback,
      0x0b,
      0x20,
      0,
      0x28,
      2,
      0,
      0x21,
      1,
      ...constant(slot),
      0x20,
      1,
    ]

    for (let argument = 0; argument < 4; argument++)
      body.push(
        ...(argument < arity ? [0x20, 0, 0x28, 2, ...unsigned((argument + 1) * 4)] : constant(0)),
      )

    // A thrown call leaves the frame untouched. Successful calls publish EAX,
    // actual_return and ESP in the same order as dispatch.finish().
    body.push(
      0x10,
      0,
      0x24,
      1,
      0x20,
      1,
      0x24,
      2,
      0x20,
      0,
      ...constant(4 + stackArgs * 4),
      0x6a,
      0x24,
      0,
      // Reports are rare, but must observe the completed guest register frame.
      0x23,
      4,
      0x04,
      0x40,
      0x10,
      2,
      0x0b,
      0x0b,
    )
    bodies.push(...unsigned(body.length), ...body)
  }

  section(7, exports)
  section(10, bodies)

  return new WebAssembly.Module(Uint8Array.from(bytes))
}
