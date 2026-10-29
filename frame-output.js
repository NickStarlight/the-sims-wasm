// Copy only frames the consumer can accept; Rust keeps the newest pending frame.
export function consumeFrame(process, onFrame, packed = false) {
  const frame = packed && process.takePackedFrame ? process.takePackedFrame() : process.takeFrame()

  if (!frame) return false

  let failed = true

  try {
    onFrame({
      width: frame.width,
      height: frame.height,
      format: frame.format ?? 32,
      pitch: frame.pitch ?? frame.width * 4,
      pixels: frame.pixels(),
    })
    failed = false
  } finally {
    try {
      frame.free()
    } catch (error) {
      if (!failed) throw error
    }
  }

  return true
}

// At most one transferred frame awaits a paint acknowledgement. Acknowledging
// immediately drains the latest pending frame, including when the guest is idle.
export function createFramePresenter(onFrame) {
  let transferred
  let packed = false
  let owner
  let available = true
  let closed = false

  function present(process) {
    if (closed) return

    owner = process

    if (!available) return

    available = false

    const consumed = consumeFrame(owner, onFrame, packed)

    if (!consumed) available = true
    else transferred?.()

    return consumed
  }

  return {
    onTransfer(callback) {
      transferred = callback
    },
    setPacked(value) {
      packed = Boolean(value)
    },
    present,
    acknowledge() {
      if (closed || available) return

      available = true

      if (owner) present(owner)
    },
    close() {
      closed = true
      owner = undefined
      transferred = undefined
    },
  }
}
