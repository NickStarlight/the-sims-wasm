import { virtualKey } from './keyboard.js'
import { createFrameRenderer } from './frame-renderer.js'

export function createDisplay(canvas, sendInput, frameStatus) {
  let renderer
  const getRenderer = () => (renderer ??= createFrameRenderer(canvas))
  let pendingFrame
  let paintScheduled = false
  let generation = 0
  let hasFrame = false
  const pressedKeys = new Set()
  const touches = new Map()
  let rightTouch = false
  let touchPosition
  let touchButtons = 0

  function releaseTouches() {
    if (touchPosition) sendInput({ kind: 'mouse', ...touchPosition, buttons: 0 })

    touchButtons = 0
    touches.clear()
    rightTouch = false
    touchPosition = undefined
  }

  function paint() {
    paintScheduled = false

    if (!pendingFrame || pendingFrame.generation !== generation) return

    const frame = pendingFrame
    const { width, height } = frame
    pendingFrame = null
    const firstFrame = !hasFrame

    if (canvas.width !== width) canvas.width = width

    if (canvas.height !== height) canvas.height = height

    getRenderer().paint(frame)
    hasFrame = true
    frameStatus.textContent = 'Game frame received.'

    if (firstFrame) canvas.focus()

    sendInput({ kind: 'frame-ready' })
  }

  function position(clientX, clientY) {
    const bounds = canvas.getBoundingClientRect()

    if (!bounds.width || !bounds.height) return

    return {
      x: Math.floor(((clientX - bounds.left) * canvas.width) / bounds.width),
      y: Math.floor(((clientY - bounds.top) * canvas.height) / bounds.height),
    }
  }

  // Use the complete native touch list on iPad; pointer events remain for mice and pens.
  for (const type of ['touchstart', 'touchmove', 'touchend', 'touchcancel']) {
    canvas.addEventListener(
      type,
      (event) => {
        event.preventDefault()

        if (type === 'touchcancel') {
          releaseTouches()

          return
        }

        const active = new Set()

        for (const touch of event.targetTouches) {
          const point = position(touch.clientX, touch.clientY)

          if (!point) continue

          active.add(touch.identifier)
          touches.set(touch.identifier, point)
        }

        // Preserve the first finger's position on lift instead of jumping to the other finger.
        touchPosition = touches.values().next().value ?? touchPosition

        for (const id of touches.keys()) {
          if (!active.has(id)) touches.delete(id)
        }

        if (touches.size >= 2) rightTouch = true

        const buttons = touches.size >= 2 ? 2 : rightTouch || touches.size === 0 ? 0 : 1

        if (touchPosition) {
          if (touchButtons === 1 && buttons === 2) {
            sendInput({ kind: 'mouse', ...touchPosition, buttons: 0 })
          }

          if (sendInput({ kind: 'mouse', ...touchPosition, buttons })) {
            touchButtons = buttons

            if (type === 'touchstart') canvas.focus({ preventScroll: true })
          }
        }

        if (touches.size === 0) releaseTouches()
      },
      { passive: false },
    )
  }

  for (const type of ['gesturestart', 'gesturechange', 'gestureend']) {
    canvas.addEventListener(type, (event) => event.preventDefault(), { passive: false })
  }

  for (const type of [
    'pointermove',
    'pointerdown',
    'pointerup',
    'pointercancel',
    'lostpointercapture',
  ]) {
    canvas.addEventListener(type, (event) => {
      if (event.pointerType === 'touch' || touches.size) return

      const point = position(event.clientX, event.clientY)

      if (!point) return

      const buttons = type === 'pointercancel' || type === 'lostpointercapture' ? 0 : event.buttons

      if (!sendInput({ kind: 'mouse', ...point, buttons })) return

      if (type === 'pointerdown') {
        canvas.focus({ preventScroll: true })
        canvas.setPointerCapture(event.pointerId)
      }

      event.preventDefault()
    })
  }

  canvas.addEventListener('contextmenu', (event) => event.preventDefault())

  for (const type of ['keydown', 'keyup']) {
    canvas.addEventListener(type, (event) => {
      const down = type === 'keydown'
      const code = virtualKey(event)

      if (code === undefined) return

      if (!sendInput({ kind: 'key', code, down, repeat: event.repeat })) return

      if (down) pressedKeys.add(code)
      else pressedKeys.delete(code)

      event.preventDefault()
    })
  }

  canvas.addEventListener('blur', () => {
    releaseTouches()

    for (const code of pressedKeys) sendInput({ kind: 'key', code, down: false, repeat: false })

    pressedKeys.clear()
  })

  return {
    get packedFrames() {
      return getRenderer().packed
    },
    present(frame) {
      pendingFrame = { ...frame, generation }

      if (paintScheduled) return

      paintScheduled = true
      requestAnimationFrame(paint)
    },
    reset() {
      releaseTouches()
      generation++
      pressedKeys.clear()
      hasFrame = false
      renderer?.clear()
      frameStatus.textContent = 'No game frame received.'
      pendingFrame = null
    },
  }
}
