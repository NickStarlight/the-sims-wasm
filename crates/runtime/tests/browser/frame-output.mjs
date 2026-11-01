import assert from 'node:assert/strict';

export async function frameOutputSmoke(directory) {
  const { consumeFrame, createFramePresenter } = await import(new URL('./display/frame-output.js', directory));
  let pending, copies = 0, takes = 0, frees = 0;
  const process = { takeFrame() { takes++; const frame = pending; pending = undefined; return frame; } };
  const frame = id => ({ width: 1, height: 1, pixels() { copies++; return Uint8Array.of(id, 0, 0, 255); }, free() { frees++; } });
  const seen = [];
  const presenter = createFramePresenter(f => seen.push(f.pixels[0]));
  let transfers = 0; presenter.onTransfer(() => transfers++);
  presenter.present(process); // no frame must not consume a credit
  pending = frame(1); presenter.present(process);
  assert.deepEqual(seen, [1]);
  const before = takes;
  for (let id = 2; id <= 10; id++) { pending = frame(id); presenter.present(process); }
  assert.equal(takes, before, 'blocked consumers must not take or copy frames');
  assert.equal(copies, 1);
  presenter.acknowledge(); // no new guest API call: still deliver the last queued frame
  assert.deepEqual(seen, [1, 10]);
  assert.equal(copies, 2); assert.equal(frees, 2);
  presenter.acknowledge(); // no queued frame; next present gets the credit
  pending = frame(11); presenter.present(process);
  assert.deepEqual(seen, [1, 10, 11]);
  assert.equal(transfers, 3, 'observe actual transfers including acknowledgement drains, not blocked attempts');
  presenter.close();
  process.takeFrame = () => { throw new Error('process freed'); };
  presenter.acknowledge(); presenter.present(process);
  assert.deepEqual(seen, [1, 10, 11]);
  let released = false;
  assert.throws(() => consumeFrame({ takeFrame: () => ({ width: 1, height: 1,
    pixels: () => new Uint8Array(4), free: () => { released = true; },
  }) }, () => { throw new Error('transport failed'); }), /transport failed/);
  assert.ok(released, 'transport errors must release owned Wasm frames');

  const trap = new WebAssembly.RuntimeError('frame pixel conversion trapped');
  const borrowed = new Error('attempted to take ownership of Rust value while it was borrowed');
  let trappedFrameFreed = false;
  assert.throws(() => consumeFrame({ takeFrame: () => ({
    width: 1, height: 1,
    pixels() { throw trap; },
    free() { trappedFrameFreed = true; throw borrowed; },
  }) }, () => assert.fail('trapped frame must not be delivered')), error => error === trap);
  assert.ok(trappedFrameFreed, 'attempt cleanup without replacing the original Wasm trap');
  assert.throws(() => consumeFrame({ takeFrame: () => ({
    width: 1, height: 1, pixels: () => new Uint8Array(4),
    free() { throw borrowed; },
  }) }, () => {}), error => error === borrowed, 'cleanup errors surface when no earlier error exists');

  let packedFreed = false, packedTakes = 0;
  const packets = [];
  const packedPresenter = createFramePresenter(frame => packets.push(frame));
  packedPresenter.setPacked(true);
  packedPresenter.present({
    takePackedFrame() { packedTakes++; return { width: 3, height: 2, pitch: 8, format: 16,
      pixels: () => new Uint8Array(16), free() { packedFreed = true; } }; },
    takeFrame() { assert.fail('packed consumer must not request RGBA conversion'); },
  });
  assert.equal(packedTakes, 1); assert.ok(packedFreed);
  assert.equal(packets[0].format, 16); assert.equal(packets[0].pitch, 8);
  assert.equal(packets[0].pixels.byteLength, 16);
  packedPresenter.close();
  let legacyFrame;
  consumeFrame({ takeFrame: () => frame(5) }, value => { legacyFrame = value; }, true);
  assert.equal(legacyFrame.format, 32, 'older runtimes retain RGBA compatibility');
  assert.equal(legacyFrame.pitch, 4);

  const { createDisplay } = await import(new URL('./display/presentation.js', directory));
  let clock = 100, clockCalls = 0, timedPending;
  const timedFrames = [];
  const timedOwner = {takeFrame() {const result = timedPending; timedPending = undefined; return result;}};
  const timed = createFramePresenter(value => timedFrames.push(value), () => {clockCalls++; return clock;});
  timed.setDiagnostics(true);
  timedPending = frame(20); timed.present(timedOwner);
  assert.deepEqual(timedFrames[0].diagnostics, {
    workerFrameIntervalMs: null, frameCreditWaitMs: null,
    workerAfterCreditMs: null, deliverySource: 'guest',
  });
  clock = 112; timed.acknowledge();
  clock = 150; timedPending = frame(21); timed.present(timedOwner);
  assert.deepEqual(timedFrames[1].diagnostics, {
    workerFrameIntervalMs: 50, frameCreditWaitMs: 12,
    workerAfterCreditMs: 38, deliverySource: 'guest',
  });
  clock = 155; timedPending = frame(22); timed.present(timedOwner);
  assert.equal(clockCalls, 3, 'blocked API calls add no timer reads or frame copies');
  clock = 162; timed.acknowledge();
  assert.deepEqual(timedFrames[2].diagnostics, {
    workerFrameIntervalMs: 12, frameCreditWaitMs: 12,
    workerAfterCreditMs: 0, deliverySource: 'acknowledgement',
  });
  timed.setDiagnostics(false);
  const beforeDisabled = clockCalls;
  clock = 200; timed.acknowledge();
  timedPending = frame(23); timed.present(timedOwner);
  assert.equal(timedFrames[3].diagnostics, undefined);
  assert.equal(clockCalls, beforeDisabled, 'disabled diagnostics do not read clocks');
  timed.setDiagnostics(true);
  clock = 250; timed.acknowledge();
  timedPending = frame(24); timed.present(timedOwner);
  assert.equal(timedFrames[4].diagnostics.workerFrameIntervalMs, null, 'a reset clears emission history');
  assert.equal(timedFrames[4].diagnostics.frameCreditWaitMs, null, 'a reset clears credit history');
  timed.close();

  const previousRaf = globalThis.requestAnimationFrame, previousImage = globalThis.ImageData;
  let paint, painted = 0;
  globalThis.requestAnimationFrame = fn => { paint = fn; };
  globalThis.ImageData = class { constructor(pixels, width, height) { Object.assign(this, { pixels, width, height }); } };
  try {
    const messages = [];
    const canvas = { width: 1, height: 1, addEventListener() {}, focus() {},
      getContext: () => ({ putImageData() { painted++; }, clearRect() {} }),
    };
    const display = createDisplay(canvas, message => { messages.push(message); return true; }, {});
    display.present({ width: 1, height: 1, pixels: new Uint8Array(4) });
    assert.deepEqual(messages, [], 'delivery is acknowledged after paint, not receipt');
    paint(); assert.equal(painted, 1); assert.deepEqual(messages, [{ kind: 'frame-ready' }]);
    display.present({ width: 1, height: 1, pixels: new Uint8Array(4) });
    display.reset(); paint();
    assert.equal(painted, 1); assert.equal(messages.length, 1, 'reset must not acknowledge a stale frame');
  } finally {
    globalThis.requestAnimationFrame = previousRaf; globalThis.ImageData = previousImage;
  }
  console.log('Frame delivery: bounded copies, latest pending frame, paint acknowledgement and shutdown passed');
}
