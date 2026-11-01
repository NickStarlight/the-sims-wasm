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
  return new PerformanceSample(now, windowMs)
}

class PerformanceSample {
  constructor(now = epochNow, windowMs = 15000) {
    Object.assign(this, { now, windowMs })
    this.started = this.now()
    this.frames = []
    this.runtimeRecords = []
    this.previousFrame = undefined
    this.lastInput = 0
  }

  reset = () => {
    this.started = this.now()
    this.frames = []
    this.runtimeRecords = []
    this.previousFrame = undefined
    this.lastInput = 0
  }

  frame = (record) => {
    const at = record.atMs ?? this.now()
    const input =
      record.input?.sequence > this.lastInput && record.input.sentAtMs >= this.started
        ? record.input
        : null

    if (input) this.lastInput = input.sequence

    this.frames.push({
      at,
      interval: this.previousFrame === undefined ? undefined : at - this.previousFrame,
      paint: record.paintMs,
      pageWait: at - record.receivedAtMs - record.paintMs,
      workerInterval: record.workerFrameIntervalMs,
      creditWait: record.frameCreditWaitMs,
      afterCredit: record.workerAfterCreditMs,
      deliverySource: record.deliverySource,
      delivery: record.receivedAtMs - record.sentAtMs,
      inputDelivery: input ? input.appliedAtMs - input.sentAtMs : undefined,
      inputFrame: input ? at - input.sentAtMs : undefined,
    })
    this.previousFrame = at
  }

  runtime = (record) => {
    if (record.atMs >= this.started) this.runtimeRecords.push(record)
  }

  snapshot = () => {
    const at = this.now()
    const from = Math.max(this.started, at - this.windowMs)
    const elapsed = at - from
    this.frames = this.frames.filter((frame) => frame.at > from)

    // Keep one baseline before the window for counter deltas.
    while (this.runtimeRecords.length > 2 && this.runtimeRecords[1].atMs <= from)
      this.runtimeRecords.shift()

    const first = this.runtimeRecords[0]
    const last = this.runtimeRecords.at(-1)
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
      framesSubmitted: this.frames.length,
      gameFramesPerSecond: elapsed > 0 ? round((this.frames.length * 1000) / elapsed) : null,
      frameIntervalMs: distribution(this.frames.map((frame) => frame.interval)),
      canvasSubmitMs: distribution(this.frames.map((frame) => frame.paint)),
      pageWaitToSubmitMs: distribution(this.frames.map((frame) => frame.pageWait)),
      workerFrameIntervalMs: distribution(this.frames.map((frame) => frame.workerInterval)),
      frameCreditWaitMs: distribution(this.frames.map((frame) => frame.creditWait)),
      workerAfterCreditMs: distribution(this.frames.map((frame) => frame.afterCredit)),
      deliverySource: {
        guest: this.frames.filter((frame) => frame.deliverySource === 'guest').length,
        acknowledgement: this.frames.filter((frame) => frame.deliverySource === 'acknowledgement')
          .length,
      },
      clockSamplesRejected: {
        workerToPage: this.frames.filter((frame) => frame.delivery < 0).length,
        inputDelivery: this.frames.filter((frame) => frame.inputDelivery < 0).length,
        inputToNextFrame: this.frames.filter((frame) => frame.inputFrame < 0).length,
      },
      workerToPageMs: distribution(this.frames.map((frame) => frame.delivery)),
      inputDeliveryMs: distribution(this.frames.map((frame) => frame.inputDelivery)),
      inputToNextFrameEstimateMs: distribution(this.frames.map((frame) => frame.inputFrame)),
      runtime: runtimeSample,
    }
  }
}

export function createPerformancePanel(display, send) {
  return new PerformancePanel(display, send)
}

class PerformancePanel {
  constructor(display, send) {
    Object.assign(this, { display, send })
    this.panel = document.querySelector('#performance-panel')
    this.toggle = document.querySelector('#performance-toggle')
    this.output = document.querySelector('#performance-values')
    this.status = document.querySelector('#performance-status')
    this.textarea = document.querySelector('#performance-report')
    this.label = document.querySelector('#performance-label')
    this.sample = createPerformanceSample()
    this.enabled = false
    this.phases = []
    this.clientId = undefined
    this.receivedAt = undefined
    this.bindEvents()
  }

  bindEvents() {
    this.toggle.addEventListener('click', this.onOpen)
    document.querySelector('#performance-hide').addEventListener('click', this.onHide)
    document.querySelector('#performance-reset').addEventListener('click', this.resetSample)
    document.querySelector('#performance-copy').addEventListener('click', this.copyReport)
    document.addEventListener('keydown', this.onShortcut, true)
    document.addEventListener('visibilitychange', this.onVisibility)
  }

  report = () => {
    return {
      version: 2,
      label: this.label.value.trim(),
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
        renderer: this.display.rendererInfo,
      },
      clientId: this.clientId,
      measurement: this.sample.snapshot(),
      phases: this.phases,
      notes: [
        'Trailing 15 seconds since opening or resetting. Game FPS counts canvas submissions, not simulation ticks or display refreshes.',
        'Canvas submission time excludes GPU completion and physical display latency.',
        'Input samples use the latest applied input per frame. The next frame may not visibly reflect that input.',
        'Runtime counters describe the main guest thread; suspended time is not total CPU usage.',
        'Frame credit wait is the worker-measured time until the page acknowledges its last frame. Worker after-credit time is the subsequent wait for another transferable frame.',
        'Cross-context clocks may be rounded differently. Negative delay samples are omitted and counted in clockSamplesRejected.',
      ],
    }
  }

  refresh = () => {
    const measurement = this.sample.snapshot()
    const value = (number, suffix = '') => (number == null ? '—' : `${number.toFixed(1)}${suffix}`)
    this.output.textContent = [
      `Game frames/s: ${value(measurement.gameFramesPerSecond)}`,
      `Frame interval p50 / p95: ${value(measurement.frameIntervalMs?.p50)} / ${value(measurement.frameIntervalMs?.p95, ' ms')}`,
      `Canvas submit p95: ${value(measurement.canvasSubmitMs?.p95, ' ms')}`,
      `Page wait to submit p95: ${value(measurement.pageWaitToSubmitMs?.p95, ' ms')}`,
      `Worker frame interval p95: ${value(measurement.workerFrameIntervalMs?.p95, ' ms')}`,
      `Frame credit wait p95: ${value(measurement.frameCreditWaitMs?.p95, ' ms')}`,
      `Worker → page p95: ${value(measurement.workerToPageMs?.p95, ' ms')}`,
      `Input delivery p95: ${value(measurement.inputDeliveryMs?.p95, ' ms')}`,
      `Input → next frame p95*: ${value(measurement.inputToNextFrameEstimateMs?.p95, ' ms')}`,
      `Main guest calls/s: ${value(measurement.runtime?.mainGuestCallsPerSecond)}`,
      `Window: ${value(measurement.windowSeconds, ' s')} · ${measurement.framesSubmitted} frames`,
    ].join('\n')
    this.send({ kind: 'performance-poll' })
  }

  resetSample = () => {
    this.sample.reset()
    this.textarea.hidden = true
    this.textarea.value = ''
    this.status.textContent = ''
    this.send({ kind: 'performance-reset' })
    this.refresh()
  }

  show = (value) => {
    this.panel.hidden = !value
    this.toggle.hidden = value

    if (!value || this.enabled) return

    this.enabled = true
    this.send({ kind: 'performance', enabled: true })
    this.resetSample()
    setInterval(this.refresh, 1000)
  }

  open = () => {
    this.show(true)
  }

  program = (id) => {
    this.clientId = id

    if (this.enabled) this.resetSample()
  }

  phase = (phase) => {
    this.phases.push(phase)
  }

  receive = () => {
    if (this.enabled) this.receivedAt = epochNow()
  }

  paint = (frame, paintMs) => {
    if (!this.enabled) return

    this.sample.frame({
      atMs: epochNow(),
      receivedAtMs: this.receivedAt,
      paintMs,
      sentAtMs: frame.diagnostics?.sentAtMs,
      input: frame.diagnostics?.input,
      workerFrameIntervalMs: frame.diagnostics?.workerFrameIntervalMs,
      frameCreditWaitMs: frame.diagnostics?.frameCreditWaitMs,
      workerAfterCreditMs: frame.diagnostics?.workerAfterCreditMs,
      deliverySource: frame.diagnostics?.deliverySource,
    })
  }

  runtime = (record) => {
    if (this.enabled) this.sample.runtime(record)
  }

  reset = () => {
    this.phases = []
    this.clientId = undefined

    if (this.enabled) this.resetSample()
  }

  onOpen = () => this.show(true)

  onHide = () => this.show(false)

  copyReport = async () => {
    const text = JSON.stringify(this.report(), null, 2)

    try {
      await navigator.clipboard.writeText(text)
      this.status.textContent = 'Copied. Paste the report in chat.'
    } catch {
      this.textarea.value = text
      this.textarea.hidden = false
      this.textarea.focus()
      this.textarea.select()
      this.status.textContent = 'Copy the selected report below.'
    }
  }

  onShortcut = (event) => {
    if (!event.altKey || !event.shiftKey || event.code !== 'KeyP') return

    event.preventDefault()
    event.stopPropagation()
    this.show(this.panel.hidden)
  }

  onVisibility = () => {
    if (this.enabled) this.resetSample()
  }
}
