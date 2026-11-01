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
export function createFramePresenter(onFrame, now = () => performance.now()) {
  return new FramePresenter(onFrame, now)
}

class FramePresenter {
  constructor(onFrame, now = () => performance.now()) {
    Object.assign(this, { onFrame, now })
    this.transferred = undefined
    this.packed = false
    this.owner = undefined
    this.available = true
    this.closed = false
    this.diagnostics = false
    this.lastTransfer = undefined
    this.lastCredit = undefined
    this.creditWait = undefined
    this.acknowledging = false
  }

  deliver = (frame) => {
    if (this.diagnostics) {
      const at = this.now()
      frame.diagnostics = {
        workerFrameIntervalMs: this.lastTransfer === undefined ? null : at - this.lastTransfer,
        frameCreditWaitMs: this.creditWait ?? null,
        workerAfterCreditMs: this.lastCredit === undefined ? null : at - this.lastCredit,
        deliverySource: this.acknowledging ? 'acknowledgement' : 'guest',
      }
      this.lastTransfer = at
    }

    this.onFrame(frame)
  }

  present = (process) => {
    if (this.closed) return

    this.owner = process

    if (!this.available) return

    this.available = false

    const consumed = consumeFrame(this.owner, this.deliver, this.packed)

    if (!consumed) this.available = true
    else this.transferred?.()

    return consumed
  }

  onTransfer = (callback) => {
    this.transferred = callback
  }

  setPacked = (value) => {
    this.packed = Boolean(value)
  }

  setDiagnostics = (value) => {
    this.diagnostics = Boolean(value)
    this.lastTransfer = this.lastCredit = this.creditWait = undefined
  }

  acknowledge = () => {
    if (this.closed || this.available) return

    this.available = true

    if (this.diagnostics) {
      this.lastCredit = this.now()
      this.creditWait =
        this.lastTransfer === undefined ? undefined : this.lastCredit - this.lastTransfer
    }

    this.acknowledging = true

    try {
      if (this.owner) this.present(this.owner)
    } finally {
      this.acknowledging = false
    }
  }

  close = () => {
    this.closed = true
    this.owner = undefined
    this.transferred = undefined
  }
}
