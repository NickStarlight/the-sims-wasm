import assert from 'node:assert/strict';

export async function syncImportsSmoke(directory) {
  const { installSynchronousImports } = await import(new URL('./sync-imports.js', directory));
  const { attachImports } = await import(new URL('./linker.js', directory));
  const noop = new WebAssembly.Instance(new WebAssembly.Module(Uint8Array.of(
    0,97,115,109,1,0,0,0,
    1,4,1,0x60,0,0,
    3,2,1,0,
    7,5,1,1,102,0,0,
    10,4,1,2,0,0x0b,
  ))).exports.f;
  const makeTable = length => {
    const table = new WebAssembly.Table({ element: 'anyfunc', initial: length });
    for (let i = 0; i < length; i++) table.set(i, noop);
    return table;
  };
  const bindings = Array.from({ length: 300 }, () => ({ api: {} }));
  const excluded = [0, 1, 2, 3, 4, 5, 6];
  bindings[0] = { api: null };
  bindings[1] = null;
  bindings[2] = { label: 'missing!unknown' };
  for (const [index, flag] of ['schedule', 'callback', 'exception', 'media'].entries())
    bindings[index + 3] = { api: { [flag]: true } };
  bindings[7] = { api: { clock: true, wallClock: 'utc', storage: true, wake: true } };
  bindings[8] = { api: { schedule: false, callback: false, exception: false, media: false } };
  const imports = { bindings, trap: 400, aliases: [[1, 400 + 23], [2, 400 + 19]] };
  const first = makeTable(800), second = makeTable(800);
  const firstCalls = [], secondCalls = [];
  // Cache compiled code only: every installation must bind a fresh thread-local dispatcher.
  const OriginalModule = WebAssembly.Module;
  let compilations = 0;
  WebAssembly.Module = new Proxy(OriginalModule, {
    construct(target, args) { compilations++; return Reflect.construct(target, args); },
  });
  try {
    assert.equal(installSynchronousImports(first, imports, slot => firstCalls.push(slot)), 293);
    assert.equal(installSynchronousImports(second, imports, slot => secondCalls.push(slot)), 293);
    assert.equal(compilations, 1);
    assert.notEqual(first.get(423), second.get(423), 'thread-specific Wasm instances must not share closures');
    for (const index of excluded) {
      assert.equal(first.get(400 + index + 16), noop, `excluded import ${index}`);
      assert.equal(second.get(400 + index + 16), noop);
    }
    for (const slot of [23, 24, 63, 64, 127, 128, 255, 256, 315]) {
      first.get(400 + slot)(); second.get(400 + slot)();
    }
    const slots = [23, 24, 63, 64, 127, 128, 255, 256, 315];
    assert.deepEqual(firstCalls, slots, 'i32.const uses signed LEB at the 64 and 128 boundaries');
    assert.deepEqual(secondCalls, slots);
    attachImports(first, imports);
    first.get(1)(); first.get(2)();
    assert.deepEqual(firstCalls, [...slots, 23], 'aliases copy installed synchronous entries, async entries stay unchanged');
    assert.equal(first.get(0), noop, 'unknown guest call handler is unchanged');
    assert.deepEqual(secondCalls, slots);

    // Classification changes must not reuse an obsolete cached module.
    bindings[7].api.media = true;
    const third = makeTable(800);
    assert.equal(installSynchronousImports(third, imports, () => {}), 292);
    assert.equal(third.get(423), noop);
    assert.equal(compilations, 2);
  } finally { WebAssembly.Module = OriginalModule; }

  const tooSmall = makeTable(700);
  assert.throws(() => installSynchronousImports(tooSmall, imports, () => {}), /function table/);
  assert.equal(tooSmall.get(424), noop, 'bounds rejection must not partially replace earlier entries');
  for (const trap of [-1, 1.5, NaN, Infinity])
    assert.throws(() => installSynchronousImports(first, { ...imports, trap }, () => {}), /base/);
  assert.throws(() => installSynchronousImports(first, { bindings: null, trap: 0 }, () => {}), /bindings/);
  assert.equal(installSynchronousImports(makeTable(0), { bindings: [], trap: 0 }, () => {}), 0);
  assert.equal(installSynchronousImports(first, { bindings: [{ api: { schedule: true } }], trap: 0 }, () => {}), 0);
  const throwing = makeTable(17);
  installSynchronousImports(throwing, { bindings: [{ api: {} }], trap: 0 }, () => { throw new Error('thread stopped'); });
  assert.throws(() => throwing.get(16)(), /thread stopped/, 'dispatcher failures propagate through the Wasm wrapper');
  const capacity = makeTable(16384), highCalls = [];
  assert.equal(installSynchronousImports(capacity, {
    bindings: Array.from({ length: 16368 }, () => ({ api: {} })), trap: 0,
  }, slot => highCalls.push(slot)), 16368);
  for (const slot of [8191, 8192, 16383]) capacity.get(slot)();
  assert.deepEqual(highCalls, [8191, 8192, 16383], 'the full linker capacity must encode wide section sizes and signed slot constants');
  console.log('Synchronous import adapters: Wasm table entries, eligibility, signed slots, module cache, thread isolation, aliases and bounds passed');
  await scheduledImportsSmoke(directory, makeTable, noop);
}

async function scheduledImportsSmoke(directory, makeTable, noop) {
  const { installScheduledImports } = await import(new URL('./sync-imports.js', directory));
  const bindings = Array.from({ length: 300 }, () => ({ api: { schedule: true } }));
  bindings[0] = { api: {} }; bindings[1] = { api: null };
  for (const [index, flag] of ['callback', 'exception', 'media'].entries())
    bindings[index + 2] = { api: { schedule: true, [flag]: true } };
  const table = makeTable(316), calls = [];
  assert.equal(installScheduledImports(table, { bindings, trap: 0 }, slot => {
    calls.push(slot); return 0;
  }, () => assert.fail('an immediate return must not invoke the suspending import')), 295);
  for (let slot = 16; slot < 21; slot++) assert.equal(table.get(slot), noop);
  for (const slot of [21, 63, 64, 127, 128, 315]) table.get(slot)();
  assert.deepEqual(calls, [21, 63, 64, 127, 128, 315]);
  const small = makeTable(300);
  assert.throws(() => installScheduledImports(small, { bindings, trap: 0 }, () => 0, () => {}), /function table/);
  assert.equal(small.get(21), noop);

  // Exercise the actual dispatcher preflight/consume pair through JSPI, not a mock Promise bridge.
  const { ApiDispatcher } = await import(new URL('./dispatch.js', directory));
  const scheduled = [{ label: 'kernel32.dll!EnterCriticalSection', api: {
    id: 1, arity: 1, stackArgs: 1, schedule: true, dll: 'kernel32.dll', name: 'EnterCriticalSection',
  } }];
  let selected;
  const process = { setThread(id) { selected = id; } };
  const makeThread = id => {
    const memory = new WebAssembly.Memory({ initial: 1 });
    const registers = Object.fromEntries(['esp', 'eax', 'actual_return'].map(name =>
      [name, new WebAssembly.Global({ value: 'i32', mutable: true }, 0)]));
    let action = () => 37;
    const dispatcher = new ApiDispatcher({ process, memory, registers, threadId: id },
      { yields: 0 }, () => {}, () => {});
    dispatcher.bindings = scheduled;
    dispatcher.threads = { signal: new AbortController().signal, schedule() { return action(); } };
    const reset = () => {
      registers.esp.value = 0x1000;
      const view = new DataView(memory.buffer);
      view.setUint32(0x1000, 0x12345678, true); view.setUint32(0x1004, 0x2000, true);
    };
    reset();
    const table = makeTable(17);
    installScheduledImports(table, { bindings: scheduled, trap: 0 },
      slot => dispatcher.dispatchSchedule(slot), () => dispatcher.awaitSchedule());
    return { dispatcher, registers, reset, run: table.get(16), setAction(fn) { action = fn; } };
  };
  const first = makeThread(7), second = makeThread(9);
  first.run();
  assert.equal(first.registers.eax.value, 37);
  assert.equal(first.registers.esp.value, 0x1008);
  assert.equal(first.dispatcher.pendingSchedule, null);
  let resumeFirst, resumeSecond;
  first.reset(); second.reset();
  first.setAction(() => new Promise(resolve => { resumeFirst = resolve; }));
  second.setAction(() => new Promise(resolve => { resumeSecond = resolve; }));
  let firstDone = false;
  const pendingFirst = WebAssembly.promising(first.run)().then(() => { firstDone = true; });
  const pendingSecond = WebAssembly.promising(second.run)();
  assert.equal(first.dispatcher.pendingSchedule, null, 'await import clears pending state before suspension');
  assert.equal(second.dispatcher.pendingSchedule, null);
  assert.equal(first.registers.esp.value, 0x1000, 'a waiting call has not returned to the guest');
  resumeSecond(92); await pendingSecond;
  assert.equal(selected, 9); assert.equal(second.registers.eax.value, 92);
  assert.equal(firstDone, false, 'one thread completion must not resume the other');
  resumeFirst(71); await pendingFirst;
  assert.equal(selected, 7); assert.equal(first.registers.eax.value, 71);
  assert.equal(first.registers.actual_return.value, 0x12345678);

  first.reset(); first.setAction(() => { throw new Error('schedule failed'); });
  assert.throws(first.run, /schedule failed/);
  assert.equal(first.dispatcher.pendingSchedule, null);
  assert.equal(first.registers.esp.value, 0x1000);
  first.setAction(() => Promise.reject(new Error('wait cancelled')));
  await assert.rejects(WebAssembly.promising(first.run)(), /wait cancelled/);
  assert.equal(first.dispatcher.pendingSchedule, null);
  first.setAction(() => 18); first.run();
  assert.equal(first.registers.eax.value, 18, 'rejected waits must not leak pending state into the next call');
  assert.throws(() => first.dispatcher.awaitSchedule(), /pending/);
  console.log('Conditional schedule adapters: immediate returns, true JSPI waits, errors, thread isolation and pending-state cleanup passed');
}
