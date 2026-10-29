// The raw Rust import receives an actual Wasm function, not a JS forwarding
// closure. A private table selects a helper with distinct guest/runtime memories.
// Registration/release crosses JS; data accesses stay inside Wasm. Memory growth
// retains the existing host call so custom grow errors keep their semantics.
// ABI: (handle, operation, a, b, c, d) -> f64. Results are unsigned DWORDs or
// lengths (including 4 GiB); -1 means backing bounds/OOM and -2 invalid operation.
// Operations: 0 read, 1 write, 2 guest->runtime, 3 runtime->guest, 4 fill,
// 5 overlapping guest copy, 6 DWORD update, 7 byte length.
// Rust still validates ownership/protection. DWORD updates are synchronous on
// one worker, not shared-memory atomics for concurrently executing guest threads.
const encoder = new TextEncoder()

const u = (n) => {
  const a = []

  do {
    const b = n & 127
    n >>>= 7
    a.push(b | (n ? 128 : 0))
  } while (n)

  return a
}

const str = (s) => [...u(encoder.encode(s).length), ...encoder.encode(s)]
const section = (id, data) => [id, ...u(data.length), ...data]
const type = (n) => [0x60, n, ...Array(n).fill(0x7f), 1, 0x7c]
const mod = (parts) =>
  new WebAssembly.Module(Uint8Array.from([0, 97, 115, 109, 1, 0, 0, 0, ...parts.flat()]))
const get = (n) => [0x20, n]
const set = (n) => [0x21, n]
const i = (n) => [0x41, n]

const f = (n) => {
  const b = new Uint8Array(8)
  new DataView(b.buffer).setFloat64(0, n, true)

  return [0x44, ...b]
}

const ret = (n) => [...f(n), 0x0f]
const eq = (local, value) => [...get(local), ...i(value), 0x46]
const branch = (condition, body) => [...condition, 0x04, 0x40, ...body, 0x0b]
// Widen unsigned address/length to i64 before adding. memory.size counts 64 KiB
// pages, so the comparison also represents a complete 4 GiB memory exactly.
const range = (address, length, memory) => [
  ...address,
  0xad,
  ...length,
  0xad,
  0x7c,
  0x3f,
  memory,
  0xad,
  0x42,
  16,
  0x86,
  0x56,
  0x04,
  0x40,
  ...ret(-1),
  0x0b,
]
const signature = (params) => section(1, [1, ...type(params)])
const exportAccess = () => section(7, [1, ...str('access'), 0, 0])
const bodySection = (code) => section(10, [1, ...u(code.length), ...code])

function createRouter() {
  const table = new WebAssembly.Table({ element: 'anyfunc', initial: 0 })
  const module = mod([
    section(1, [2, ...type(5), ...type(6)]),
    section(2, [1, ...str('env'), ...str('table'), 1, 0x70, 0, 0]),
    section(3, [1, 1]),
    exportAccess(),
    bodySection([0, ...[1, 2, 3, 4, 5].flatMap(get), ...get(0), 0x11, 0, 0, 0x0b]),
  ])

  return { table, access: new WebAssembly.Instance(module, { env: { table } }).exports.access }
}

const modules = new Map()

function createHelper(guest, runtime) {
  if (guest === runtime) throw new TypeError('Guest and runtime memories must be distinct.')

  // The constructor may be hidden even when shared Wasm memories are supported;
  // a buffer can also originate in another realm.
  const shared = Object.prototype.toString.call(guest.buffer) === '[object SharedArrayBuffer]'
  let module = modules.get(shared)

  if (!modules.has(shared)) {
    const code = [1, 2, 0x7f] // two temporary i32 locals
    const loadWidth = [
      ...range(get(1), get(2), 0),
      ...branch(eq(2, 1), [...get(1), 0x2d, 0, 0, 0xb8, 0x0f]),
      ...branch(eq(2, 2), [...get(1), 0x2f, 1, 0, 0xb8, 0x0f]),
      ...branch(eq(2, 4), [...get(1), 0x28, 2, 0, 0xb8, 0x0f]),
      ...ret(-2),
    ]
    code.push(...branch(eq(0, 0), loadWidth))
    const writeWidth = [
      ...range(get(1), get(2), 0),
      ...branch(eq(2, 1), [...get(1), ...get(3), 0x3a, 0, 0, ...ret(0)]),
      ...branch(eq(2, 2), [...get(1), ...get(3), 0x3b, 1, 0, ...ret(0)]),
      ...branch(eq(2, 4), [...get(1), ...get(3), 0x36, 2, 0, ...ret(0)]),
      ...ret(-2),
    ]
    code.push(...branch(eq(0, 1), writeWidth))

    for (const [operation, dest, src] of [
      [2, 1, 0],
      [3, 0, 1],
    ]) {
      code.push(
        ...branch(eq(0, operation), [
          ...range(get(1), get(3), 0),
          ...range(get(2), get(3), 1),
          ...get(dest ? 2 : 1),
          ...get(dest ? 1 : 2),
          ...get(3),
          0xfc,
          10,
          dest,
          src,
          ...ret(0),
        ]),
      )
    }

    code.push(
      ...branch(eq(0, 4), [
        ...range(get(1), get(2), 0),
        ...get(1),
        ...get(3),
        ...get(2),
        0xfc,
        11,
        0,
        ...ret(0),
      ]),
    )
    code.push(
      ...branch(eq(0, 5), [
        ...branch([...get(3), 0x45], ret(0)),
        ...range(get(1), get(3), 0),
        ...range(get(2), get(3), 0),
        ...get(1),
        ...get(2),
        ...get(3),
        0xfc,
        10,
        0,
        0,
        ...ret(0),
      ]),
    )
    code.push(
      ...branch(eq(0, 6), [
        ...range(get(1), i(4), 0),
        ...get(1),
        0x28,
        2,
        0,
        ...set(5),
        ...get(5),
        ...set(6),
        ...branch(eq(2, 0), [...get(5), ...get(3), 0x6a, ...set(6)]),
        ...branch(eq(2, 1), [...get(3), ...set(6)]),
        ...branch(eq(2, 2), [...branch([...get(5), ...get(4), 0x46], [...get(3), ...set(6)])]),
        ...branch([...get(2), ...i(2), 0x4b], ret(-2)),
        ...get(1),
        ...get(6),
        0x36,
        2,
        0,
        ...get(5),
        0xb8,
        0x0f,
      ]),
    )
    code.push(...branch(eq(0, 7), [0x3f, 0, 0xad, 0x42, 16, 0x86, 0xba, 0x0f]))
    code.push(...f(-2), 0x0b)
    const guestLimits = shared ? [3, 0, ...u(65536)] : [0, 0]
    const parts = [
      signature(5),
      section(2, [
        2,
        ...str('env'),
        ...str('guest'),
        2,
        ...guestLimits,
        ...str('env'),
        ...str('runtime'),
        2,
        0,
        0,
      ]),
      section(3, [1, 0]),
      exportAccess(),
      bodySection(code),
    ]
    const bytes = Uint8Array.from([0, 97, 115, 109, 1, 0, 0, 0, ...parts.flat()])
    module = WebAssembly.validate(bytes) ? new WebAssembly.Module(bytes) : null
    modules.set(shared, module)
  }

  return module
    ? new WebAssembly.Instance(module, { env: { guest, runtime } }).exports.access
    : null
}

const router = createRouter()
const available = []
const active = new Set()

// wasm-bindgen's standard web loader preserves this raw namespace export.
export const guestMemoryAccess = router.access

export function registerGuestMemory(guest, runtime) {
  // Check before the feature probe: fallback must also preserve memory isolation.
  if (guest === runtime) throw new TypeError('Guest and runtime memories must be distinct.')

  let helper

  try {
    helper = createHelper(guest, runtime)
  } catch (error) {
    // A successful validate() need not guarantee module compilation/linkage on
    // every engine. Preserve the existing backend for unsupported capabilities.
    if (error instanceof WebAssembly.CompileError || error instanceof WebAssembly.LinkError)
      return -1

    throw error
  }

  if (!helper) return -1

  const handle = available.length ? available.pop() : router.table.grow(1)

  try {
    router.table.set(handle, helper)
    active.add(handle)
  } catch (error) {
    router.table.set(handle, null)
    available.push(handle)
    throw error
  }

  return handle
}

export function releaseGuestMemory(handle) {
  if (!active.delete(handle)) return

  router.table.set(handle, null)
  available.push(handle)
}
