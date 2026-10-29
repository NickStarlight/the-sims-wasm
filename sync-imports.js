import { installNativeImports } from './native-imports.js'

// The module contains no thread state. Each installation captures its own dispatcher in an instance.
const modules = new WeakMap()
const scheduledModules = new WeakMap()
const encoder = new TextEncoder()

export function installSynchronousImports(table, imports, dispatchSync, dispatcher) {
  const count = install(table, imports, { dispatchSync }, false)

  if (dispatcher) installNativeImports(table, imports, dispatcher)

  return count
}

export function installScheduledImports(table, imports, dispatchSchedule, awaitSchedule) {
  return install(
    table,
    imports,
    {
      dispatchSchedule,
      awaitSchedule: new WebAssembly.Suspending(awaitSchedule),
    },
    true,
  )
}

function install(table, imports, env, scheduled) {
  const { bindings, trap } = imports

  if (!Array.isArray(bindings)) throw new TypeError('Import bindings must be an array.')

  if (!Number.isSafeInteger(trap) || trap < 0)
    throw new RangeError('Invalid import trampoline base.')

  const slots = []

  for (let index = 0; index < bindings.length; index++) {
    const api = bindings[index]?.api

    if (api && Boolean(api.schedule) === scheduled && !api.callback && !api.exception && !api.media)
      slots.push(index + 16)
  }

  // Validate before changing any entries, including when a later binding would be out of bounds.
  if (slots.length && trap + slots.at(-1) >= table.length)
    throw new RangeError('Synchronous import trampolines exceed the function table.')

  if (!slots.length) return 0

  const cache = scheduled ? scheduledModules : modules
  let cached = cache.get(bindings)

  if (
    !cached ||
    cached.slots.length !== slots.length ||
    slots.some((slot, i) => cached.slots[i] !== slot)
  ) {
    cached = { slots, module: compile(slots, scheduled) }
    cache.set(bindings, cached)
  }

  const instance = new WebAssembly.Instance(cached.module, { env })

  for (const slot of slots) table.set(trap + slot, instance.exports[`f${slot}`])

  return slots.length
}

function unsigned(value) {
  const bytes = []

  do {
    const byte = value & 0x7f
    value >>>= 7
    bytes.push(byte | (value ? 0x80 : 0))
  } while (value)

  return bytes
}

function signed(value) {
  const bytes = []

  for (;;) {
    const byte = value & 0x7f
    value >>= 7
    const done = (value === 0 && !(byte & 0x40)) || (value === -1 && byte & 0x40)
    bytes.push(byte | (done ? 0 : 0x80))

    if (done) return bytes
  }
}

function name(value) {
  const bytes = encoder.encode(value)

  return [...unsigned(bytes.length), ...bytes]
}

function compile(slots, scheduled) {
  const bytes = [0, 97, 115, 109, 1, 0, 0, 0]

  const section = (id, payload) => {
    bytes.push(id, ...unsigned(payload.length))

    // Avoid argument-count limits when a complete import table produces a large section.
    for (const byte of payload) bytes.push(byte)
  }

  // Preflight returns an i32 only for scheduled APIs; wrapper exports always remain ()->().
  section(1, [2, 0x60, 1, 0x7f, ...(scheduled ? [1, 0x7f] : [0]), 0x60, 0, 0])
  section(
    2,
    scheduled
      ? [
          2,
          ...name('env'),
          ...name('dispatchSchedule'),
          0,
          0,
          ...name('env'),
          ...name('awaitSchedule'),
          0,
          1,
        ]
      : [1, ...name('env'), ...name('dispatchSync'), 0, 0],
  )
  section(3, [...unsigned(slots.length), ...slots.map(() => 1)])
  const exports = unsigned(slots.length)
  const code = unsigned(slots.length)
  slots.forEach((slot, index) => {
    exports.push(...name(`f${slot}`), 0, ...unsigned(index + (scheduled ? 2 : 1)))
    const body = [
      0,
      0x41,
      ...signed(slot),
      0x10,
      0,
      ...(scheduled ? [0x04, 0x40, 0x10, 1, 0x0b] : []),
      0x0b,
    ]
    code.push(...unsigned(body.length), ...body)
  })
  section(7, exports)
  section(10, code)

  return new WebAssembly.Module(Uint8Array.from(bytes))
}
