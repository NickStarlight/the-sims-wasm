// Rust validates ownership and committed pages before every call. Cache only the
// backing view, never permissions; memory.grow invalidates the buffer identity.
// Guest threads share one worker: this synchronous operation never yields. It is
// not a shared-memory atomic and must change if guest threads run in parallel.
const views = new WeakMap()

export function updateU32(memory, address, operation, value, comparand) {
  const buffer = memory.buffer
  let view = views.get(memory)

  if (view?.buffer !== buffer) {
    // Omitting the explicit length also supports a complete 4 GiB memory.
    view = new DataView(buffer)
    views.set(memory, view)
  }

  if (address + 4 > view.byteLength) return -1

  const previous = view.getUint32(address, true)
  let updated

  switch (operation) {
    case 0:
      updated = (previous + value) >>> 0
      break
    case 1:
      updated = value
      break
    case 2:
      updated = previous === comparand >>> 0 ? value : previous
      break
    default:
      throw new Error('Invalid DWORD update operation.')
  }

  view.setUint32(address, updated, true)

  return previous
}
