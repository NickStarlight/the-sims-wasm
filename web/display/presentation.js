import { virtualKey } from './keyboard.js'
import { createFrameRenderer } from './frame-renderer.js'

const twoFingerGraceMs = 250

export function createDisplay(canvas, sendInput, frameStatus, onPaint) {
  return new Display(canvas, sendInput, frameStatus, onPaint)
}

class Display {
  constructor(canvas, sendInput, frameStatus, onPaint) {
    Object.assign(this, { canvas, sendInput, frameStatus, onPaint })
    this.renderer = undefined
    this.pendingFrame = undefined
    this.paintScheduled = false
    this.generation = 0
    this.hasFrame = false
    this.pressedKeys = new Set()
    this.touches = new Map()
    this.rightTouch = false
    this.touchPosition = undefined
    this.touchButtons = 0
    this.touchStartPosition = undefined
    this.touchTimer = undefined
    this.bindEvents()
  }

  bindEvents() {
    for (const type of ['touchstart', 'touchmove', 'touchend', 'touchcancel']) {
      this.canvas.addEventListener(type, (event) => this.onTouch(type, event), { passive: false })
    }

    for (const type of ['gesturestart', 'gesturechange', 'gestureend']) {
      this.canvas.addEventListener(type, this.preventGesture, { passive: false })
    }

    for (const type of [
      'pointermove',
      'pointerdown',
      'pointerup',
      'pointercancel',
      'lostpointercapture',
    ]) {
      this.canvas.addEventListener(type, (event) => this.onPointer(type, event))
    }

    this.canvas.addEventListener('contextmenu', this.preventContextMenu)

    for (const type of ['keydown', 'keyup']) {
      this.canvas.addEventListener(type, (event) => this.onKey(type, event))
    }

    this.canvas.addEventListener('blur', this.onBlur)
  }

  getRenderer = () => (this.renderer ??= createFrameRenderer(this.canvas))

  cancelTouchTimer = () => {
    clearTimeout(this.touchTimer)
    this.touchTimer = undefined
  }

  pressSingleTouch = () => {
    this.cancelTouchTimer()

    if (!this.touchStartPosition || this.rightTouch) return

    if (this.sendInput({ kind: 'mouse', ...this.touchStartPosition, buttons: 1 })) {
      this.touchButtons = 1

      if (
        this.touchPosition.x !== this.touchStartPosition.x ||
        this.touchPosition.y !== this.touchStartPosition.y
      ) {
        this.sendInput({ kind: 'mouse', ...this.touchPosition, buttons: 1 })
      }
    }
  }

  releaseTouches = () => {
    this.cancelTouchTimer()

    if (this.touchPosition) this.sendInput({ kind: 'mouse', ...this.touchPosition, buttons: 0 })

    this.touchButtons = 0
    this.touches.clear()
    this.rightTouch = false
    this.touchPosition = undefined
    this.touchStartPosition = undefined
  }

  paint = () => {
    this.paintScheduled = false

    if (!this.pendingFrame || this.pendingFrame.generation !== this.generation) return

    const frame = this.pendingFrame
    const { width, height } = frame
    this.pendingFrame = null
    const firstFrame = !this.hasFrame
    const started = this.onPaint ? performance.now() : 0

    if (this.canvas.width !== width) this.canvas.width = width

    if (this.canvas.height !== height) this.canvas.height = height

    this.getRenderer().paint(frame)
    this.onPaint?.(frame, performance.now() - started)
    this.hasFrame = true
    this.frameStatus.textContent = 'Game frame received.'

    if (firstFrame) this.canvas.focus()

    this.sendInput({ kind: 'frame-ready' })
  }

  position = (clientX, clientY) => {
    const bounds = this.canvas.getBoundingClientRect()

    if (!bounds.width || !bounds.height) return

    return {
      x: Math.floor(((clientX - bounds.left) * this.canvas.width) / bounds.width),
      y: Math.floor(((clientY - bounds.top) * this.canvas.height) / bounds.height),
    }
  }

  get rendererInfo() {
    return this.getRenderer().info
  }

  get packedFrames() {
    return this.getRenderer().packed
  }

  present = (frame) => {
    this.pendingFrame = { ...frame, generation: this.generation }

    if (this.paintScheduled) return

    this.paintScheduled = true
    requestAnimationFrame(this.paint)
  }

  reset = () => {
    this.releaseTouches()
    this.generation++
    this.pressedKeys.clear()
    this.hasFrame = false
    this.renderer?.clear()
    this.frameStatus.textContent = 'No game frame received.'
    this.pendingFrame = null
  }

  onTouch = (type, event) => {
    event.preventDefault()

    if (type === 'touchcancel') {
      this.releaseTouches()

      return
    }

    const active = new Set()

    for (const touch of event.targetTouches) {
      const point = this.position(touch.clientX, touch.clientY)

      if (!point) continue

      active.add(touch.identifier)
      this.touches.set(touch.identifier, point)
    }

    // Preserve the first finger's position on lift instead of jumping to the other finger.
    this.touchPosition = this.touches.values().next().value ?? this.touchPosition

    for (const id of this.touches.keys()) {
      if (!active.has(id)) this.touches.delete(id)
    }

    if (this.touches.size >= 2) {
      this.rightTouch = true
      this.cancelTouchTimer()
    }

    if (this.touchPosition && !this.touchStartPosition) {
      this.touchStartPosition = this.touchPosition

      // Wait for a staggered second finger before committing a left press.
      if (!this.rightTouch) this.touchTimer = setTimeout(this.pressSingleTouch, twoFingerGraceMs)
    }

    if (type === 'touchstart' && this.touchPosition) this.canvas.focus({ preventScroll: true })

    if (this.touches.size === 0) {
      // Quick single-finger taps need not wait for the recognition window.
      if (this.touchTimer !== undefined) this.pressSingleTouch()

      this.releaseTouches()

      return
    }

    const buttons = this.touches.size >= 2 ? 2 : this.rightTouch ? 0 : this.touchButtons

    if (this.touchPosition) {
      if (this.touchButtons === 1 && buttons === 2) {
        this.sendInput({ kind: 'mouse', ...this.touchPosition, buttons: 0 })
      }

      if (this.sendInput({ kind: 'mouse', ...this.touchPosition, buttons })) {
        this.touchButtons = buttons
      }
    }
  }

  preventGesture = (event) => event.preventDefault()

  onPointer = (type, event) => {
    if (event.pointerType === 'touch' || this.touches.size) return

    const point = this.position(event.clientX, event.clientY)

    if (!point) return

    const buttons = type === 'pointercancel' || type === 'lostpointercapture' ? 0 : event.buttons

    if (!this.sendInput({ kind: 'mouse', ...point, buttons })) return

    if (type === 'pointerdown') {
      this.canvas.focus({ preventScroll: true })
      this.canvas.setPointerCapture(event.pointerId)
    }

    event.preventDefault()
  }

  preventContextMenu = (event) => event.preventDefault()

  onKey = (type, event) => {
    const down = type === 'keydown'
    const code = virtualKey(event)

    if (code === undefined) return

    if (!this.sendInput({ kind: 'key', code, down, repeat: event.repeat })) return

    if (down) this.pressedKeys.add(code)
    else this.pressedKeys.delete(code)

    event.preventDefault()
  }

  onBlur = () => {
    this.releaseTouches()

    for (const code of this.pressedKeys)
      this.sendInput({ kind: 'key', code, down: false, repeat: false })

    this.pressedKeys.clear()
  }
}
