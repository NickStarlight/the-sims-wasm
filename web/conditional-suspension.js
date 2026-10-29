// Keep the common synchronous path outside JSPI; suspend only for an actual Promise.
// The tiny adapter is a Wasm function so its suspending call remains on the guest stack.
const text = new TextEncoder()

const name = (value) => {
  const bytes = text.encode(value)

  return [bytes.length, ...bytes]
}

const bytes = [0, 97, 115, 109, 1, 0, 0, 0]
const section = (id, payload) => bytes.push(id, payload.length, ...payload)
section(1, [3, 0x60, 1, 0x7f, 1, 0x7f, 0x60, 0, 0, 0x60, 1, 0x7f, 0])
section(2, [2, ...name('env'), ...name('check'), 0, 0, ...name('env'), ...name('wait'), 0, 1])
section(3, [1, 2])
section(7, [1, ...name('checkpoint'), 0, 2])
const body = [0, 0x20, 0, 0x10, 0, 0x04, 0x40, 0x10, 1, 0x0b, 0x0b]
section(10, [1, body.length, ...body])
const module = new WebAssembly.Module(Uint8Array.from(bytes))

export function suspendWhenNeeded(run) {
  let pending

  return new WebAssembly.Instance(module, {
    env: {
      check(value) {
        const result = run(value)

        if (typeof result?.then !== 'function') return 0

        if (pending) throw new Error('A checkpoint is already awaiting suspension.')

        pending = result

        return 1
      },
      wait: new WebAssembly.Suspending(() => {
        const result = pending
        pending = undefined

        return result
      }),
    },
  }).exports.checkpoint
}
