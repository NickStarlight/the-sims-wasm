const epochNow = () => performance.timeOrigin + performance.now()
const round = (value) => Math.round(value * 100) / 100

function distribution(values) {
  const sorted = values
    .filter((value) => Number.isFinite(value) && value >= 0)
    .sort((a, b) => a - b)

  if (!sorted.length) return null

  const percentile = (fraction) => round(sorted[Math.ceil(sorted.length * fraction) - 1])

  return {
    samples: sorted.length,
    mean: round(sorted.reduce((sum, value) => sum + value, 0) / sorted.length),
    p50: percentile(0.5),
    p95: percentile(0.95),
    max: round(sorted.at(-1)),
  }
}

// Measure a trailing window of actual game-frame submissions, including time
// without new frames. This never substitutes requestAnimationFrame rate for FPS.
export function createPerformanceSample(now = epochNow, windowMs = 15000) {
  let started = now()
  let frames = []
  let runtime = []
  let previousFrame
  let lastInput = 0

  return {
    reset() {
      started = now()
      frames = []
      runtime = []
      previousFrame = undefined
      lastInput = 0
    },
    frame(record) {
      const at = record.atMs ?? now()
      const input =
        record.input?.sequence > lastInput && record.input.sentAtMs >= started ? record.input : null

      if (input) lastInput = input.sequence

      frames.push({
        at,
        interval: previousFrame === undefined ? undefined : at - previousFrame,
        paint: record.paintMs,
        delivery: record.receivedAtMs - record.sentAtMs,
        inputDelivery: input ? input.appliedAtMs - input.sentAtMs : undefined,
        inputFrame: input ? at - input.sentAtMs : undefined,
      })
      previousFrame = at
    },
    runtime(record) {
      if (record.atMs >= started) runtime.push(record)
    },
    snapshot() {
      const at = now()
      const from = Math.max(started, at - windowMs)
      const elapsed = at - from
      frames = frames.filter((frame) => frame.at > from)

      // Keep one baseline before the window for counter deltas.
      while (runtime.length > 2 && runtime[1].atMs <= from) runtime.shift()

      const first = runtime[0]
      const last = runtime.at(-1)
      const duration = first && last ? last.atMs - first.atMs : 0
      const delta = (key) => (duration > 0 ? round(last[key] - first[key]) : null)
      const runtimeSample = last
        ? {
            sampledSeconds: round(duration / 1000),
            mainGuestCallsPerSecond:
              duration > 0 ? round(((last.calls - first.calls) * 1000) / duration) : null,
            mainGuestYields: delta('yields'),
            mainGuestSuspendedMs: delta('suspendedMs'),
            mainGuestPacedMs: delta('pacedMs'),
            sourceReadMs: delta('sourceReadMs'),
            sourceBytes: delta('sourceBytes'),
            activeThreads: last.activeThreads,
            pendingWaits: last.pendingWaits,
            guestMemoryBytes: last.guestMemoryBytes,
            lastCall: last.lastCall,
          }
        : null

      return {
        windowSeconds: round(elapsed / 1000),
        framesSubmitted: frames.length,
        gameFramesPerSecond: elapsed > 0 ? round((frames.length * 1000) / elapsed) : null,
        frameIntervalMs: distribution(frames.map((frame) => frame.interval)),
        canvasSubmitMs: distribution(frames.map((frame) => frame.paint)),
        workerToPageMs: distribution(frames.map((frame) => frame.delivery)),
        inputDeliveryMs: distribution(frames.map((frame) => frame.inputDelivery)),
        inputToNextFrameEstimateMs: distribution(frames.map((frame) => frame.inputFrame)),
        runtime: runtimeSample,
      }
    },
  }
}

export function createPerformancePanel(display, send) {
  const panel = document.querySelector('#performance-panel')
  const toggle = document.querySelector('#performance-toggle')
  const output = document.querySelector('#performance-values')
  const status = document.querySelector('#performance-status')
  const textarea = document.querySelector('#performance-report')
  const label = document.querySelector('#performance-label')
  const sample = createPerformanceSample()
  let enabled = false
  let phases = []
  let clientId
  let receivedAt

  function report() {
    return {
      version: 1,
      label: label.value.trim(),
      date: new Date().toISOString(),
      page: location.origin + location.pathname,
      environment: {
        browser: navigator.userAgent,
        platform: navigator.platform,
        hardwareConcurrency: navigator.hardwareConcurrency,
        deviceMemoryGiB: navigator.deviceMemory ?? null,
        touchPoints: navigator.maxTouchPoints,
        screen: { width: screen.width, height: screen.height, pixelRatio: devicePixelRatio },
        viewport: { width: innerWidth, height: innerHeight },
        visibility: document.visibilityState,
        renderer: display.rendererInfo,
      },
      clientId,
      measurement: sample.snapshot(),
      phases,
      notes: [
        'Trailing 15 seconds since opening or resetting. Game FPS counts canvas submissions, not simulation ticks or display refreshes.',
        'Canvas submission time excludes GPU completion and physical display latency.',
        'Input samples use the latest applied input per frame. The next frame may not visibly reflect that input.',
        'Runtime counters describe the main guest thread; suspended time is not total CPU usage.',
      ],
    }
  }

  function refresh() {
    const measurement = sample.snapshot()
    const value = (number, suffix = '') => (number == null ? '—' : `${number.toFixed(1)}${suffix}`)
    output.textContent = [
      `Game frames/s: ${value(measurement.gameFramesPerSecond)}`,
      `Frame interval p50 / p95: ${value(measurement.frameIntervalMs?.p50)} / ${value(measurement.frameIntervalMs?.p95, ' ms')}`,
      `Canvas submit p95: ${value(measurement.canvasSubmitMs?.p95, ' ms')}`,
      `Worker → page p95: ${value(measurement.workerToPageMs?.p95, ' ms')}`,
      `Input delivery p95: ${value(measurement.inputDeliveryMs?.p95, ' ms')}`,
      `Input → next frame p95*: ${value(measurement.inputToNextFrameEstimateMs?.p95, ' ms')}`,
      `Main guest calls/s: ${value(measurement.runtime?.mainGuestCallsPerSecond)}`,
      `Window: ${value(measurement.windowSeconds, ' s')} · ${measurement.framesSubmitted} frames`,
    ].join('\n')
    send({ kind: 'performance-poll' })
  }

  function reset() {
    sample.reset()
    textarea.hidden = true
    textarea.value = ''
    status.textContent = ''
    send({ kind: 'performance-reset' })
    refresh()
  }

  function show(value) {
    panel.hidden = !value
    toggle.hidden = value

    if (!value || enabled) return

    enabled = true
    send({ kind: 'performance', enabled: true })
    reset()
    setInterval(refresh, 1000)
  }

  toggle.addEventListener('click', () => show(true))
  document.querySelector('#performance-hide').addEventListener('click', () => show(false))
  document.querySelector('#performance-reset').addEventListener('click', reset)
  document.querySelector('#performance-copy').addEventListener('click', async () => {
    const text = JSON.stringify(report(), null, 2)

    try {
      await navigator.clipboard.writeText(text)
      status.textContent = 'Copied. Paste the report in chat.'
    } catch {
      textarea.value = text
      textarea.hidden = false
      textarea.focus()
      textarea.select()
      status.textContent = 'Copy the selected report below.'
    }
  })
  document.addEventListener(
    'keydown',
    (event) => {
      if (!event.altKey || !event.shiftKey || event.code !== 'KeyP') return

      event.preventDefault()
      event.stopPropagation()
      show(panel.hidden)
    },
    true,
  )
  document.addEventListener('visibilitychange', () => {
    if (enabled) reset()
  })

  return {
    get enabled() {
      return enabled
    },
    open() {
      show(true)
    },
    program(id) {
      clientId = id

      if (enabled) reset()
    },
    phase(phase) {
      phases.push(phase)
    },
    receive() {
      if (enabled) receivedAt = epochNow()
    },
    paint(frame, paintMs) {
      if (!enabled) return

      sample.frame({
        atMs: epochNow(),
        receivedAtMs: receivedAt,
        paintMs,
        sentAtMs: frame.diagnostics?.sentAtMs,
        input: frame.diagnostics?.input,
      })
    },
    runtime(record) {
      if (enabled) sample.runtime(record)
    },
    reset() {
      phases = []
      clientId = undefined

      if (enabled) reset()
    },
  }
}
