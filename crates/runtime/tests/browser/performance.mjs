import assert from 'node:assert/strict'

export async function performanceSmoke(directory) {
  const { createPerformanceSample } = await import(new URL('./performance.js', directory))
  let clock = 1000
  const sample = createPerformanceSample(() => clock, 1000)
  assert.equal(sample.snapshot().gameFramesPerSecond, null)
  assert.equal(sample.snapshot().frameIntervalMs, null)
  const runtime = (atMs, calls, suspendedMs) => ({
    atMs,
    calls,
    suspendedMs,
    yields: calls / 10,
    pacedMs: suspendedMs / 2,
    sourceReadMs: calls / 100,
    sourceBytes: calls * 10,
    activeThreads: 2,
    pendingWaits: 1,
    guestMemoryBytes: 16777216,
    lastCall: 'Sleep',
  })
  sample.runtime(runtime(1000, 500, 100))
  const input = { sequence: 1, sentAtMs: 1020, appliedAtMs: 1025 }
  for (const at of [1050, 1100, 1150, 1200]) {
    clock = at
    sample.frame({ atMs: at, paintMs: 2, sentAtMs: at - 8, receivedAtMs: at - 5, input })
  }
  sample.runtime(runtime(1200, 900, 150))
  let result = sample.snapshot()
  assert.equal(result.gameFramesPerSecond, 20)
  assert.equal(result.framesSubmitted, 4)
  assert.deepEqual(result.frameIntervalMs, { samples: 3, mean: 50, p50: 50, p95: 50, max: 50 })
  assert.equal(result.canvasSubmitMs.p95, 2)
  assert.equal(result.workerToPageMs.p95, 3)
  assert.deepEqual(result.inputDeliveryMs, { samples: 1, mean: 5, p50: 5, p95: 5, max: 5 })
  assert.equal(result.inputToNextFrameEstimateMs.samples, 1, 'a held input is sampled once')
  assert.equal(result.inputToNextFrameEstimateMs.p95, 30)
  assert.equal(result.runtime.mainGuestCallsPerSecond, 2000)
  assert.equal(result.runtime.mainGuestSuspendedMs, 50)
  assert.equal(result.runtime.mainGuestPacedMs, 25)
  assert.equal(result.runtime.sourceBytes, 4000)

  clock = 2000
  assert.equal(sample.snapshot().gameFramesPerSecond, 4, 'idle wall time reduces FPS')
  clock = 2200
  result = sample.snapshot()
  assert.equal(result.framesSubmitted, 0, 'window boundaries exclude old frames')
  assert.equal(result.gameFramesPerSecond, 0, 'no frames is zero FPS, not display refresh rate')
  assert.equal(result.frameIntervalMs, null)

  sample.reset()
  sample.runtime(runtime(2100, 5000, 300))
  sample.frame({ atMs: 2250, paintMs: 0.5, sentAtMs: 2260, receivedAtMs: 2250, input })
  clock = 2300
  result = sample.snapshot()
  assert.equal(result.gameFramesPerSecond, 10)
  assert.equal(result.frameIntervalMs, null, 'reset discards the previous frame baseline')
  assert.equal(result.inputDeliveryMs, null, 'pre-reset input is not counted in the new sample')
  assert.equal(
    result.workerToPageMs,
    null,
    'negative clock differences are not valid latency samples',
  )
  assert.equal(result.runtime, null, 'a late pre-reset worker response is discarded')

  // Distinguish percentile tails from averages; percentiles use nearest ranks.
  sample.reset()
  for (const interval of [10, 10, 10, 100]) {
    clock += interval
    sample.frame({ atMs: clock, paintMs: interval })
  }
  result = sample.snapshot()
  assert.equal(result.frameIntervalMs.p50, 10)
  assert.equal(result.frameIntervalMs.p95, 100)
  assert.equal(result.canvasSubmitMs.p50, 10)
  assert.equal(result.canvasSubmitMs.p95, 100)
  console.log(
    'Performance samples: wall-time FPS, p50/p95, idle windows, input deduplication, counter deltas, reset and missing data passed',
  )
}
