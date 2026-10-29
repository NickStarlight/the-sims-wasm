import assert from 'node:assert/strict';
import { interlockedSmoke, memoryUpdatesSmoke } from './interlocked.mjs';

export async function bridgeSmoke(Process, directory) {
  const { updateU32 } = await import(new URL('./memory-ops.js', directory));
  memoryUpdatesSmoke(updateU32);
  interlockedSmoke(Process);
  const memory = new WebAssembly.Memory({ initial: 4, maximum: 32 });
  const owner = new Process(memory, 0x10000, 0x200000);
  try {
    const resolve = (name, dll = 'kernel32.dll') => JSON.parse(owner.resolve(dll, name));
    const small = (api, args, now, wall) => owner.callSmall(
      api.id, args.length, args[0] ?? 0, args[1] ?? 0, args[2] ?? 0, args[3] ?? 0, now, wall,
    );
    const pointer = owner.heapAlloc(32);
    const word = value => new DataView(memory.buffer).setUint32(pointer, value, true);
    for (const [name, args] of [
      ['GetVersion', []], ['GetLastError', []], ['TlsGetValue', [0]],
      ['InterlockedIncrement', [pointer]], ['InterlockedDecrement', [pointer]],
      ['InterlockedExchange', [pointer, 0xffffffff]],
      ['InterlockedCompareExchange', [pointer, 7, 0x12345678]],
    ]) {
      const api = resolve(name);
      word(0x12345678); const expected = owner.call(api.id, Uint32Array.from(args));
      const expectedBytes = [...owner.read(pointer, 4)];
      word(0x12345678); assert.equal(small(api, args), expected, name);
      assert.deepEqual([...owner.read(pointer, 4)], expectedBytes, name);
    }
    const version = resolve('GetVersion');
    assert.throws(() => small(version, [1]), /expects 0 arguments/);
    assert.throws(() => owner.callSmall(version.id, 5, 0, 0, 0, 0), /at most four/);
    assert.throws(() => owner.callSmall(0xffffffff, 0, 0, 0, 0, 0), /unknown kernel API/);
    assert.throws(() => small(resolve('InterlockedIncrement'), [0]), /mapped|protected|range/i);
    assert.throws(() => small(version, [], 1), /non-clock API/);
    const clock = resolve('GetTickCount');
    assert.throws(() => small(clock, []), /fresh monotonic/);
    assert.equal(small(clock, [], 123.5), owner.call(clock.id, new Uint32Array(), 123.5));
    const wall = resolve('GetSystemTimeAsFileTime');
    assert.throws(() => small(wall, [pointer]), /fresh sample/);
    small(wall, [pointer], undefined, 1700000000000);
    const expectedTime = [...owner.read(pointer, 8)];
    owner.call(wall.id, Uint32Array.of(pointer), undefined, 1700000000000);
    assert.deepEqual([...owner.read(pointer, 8)], expectedTime);
    // Memory views must refresh after the scalar path grows guest memory too.
    const alloc = resolve('VirtualAlloc');
    const address = small(alloc, [0, 0x10000, 0x3000, 4]);
    assert.notEqual(address, 0);
    assert.equal(small(resolve('InterlockedIncrement'), [address]), 1);
    const scheduled = (name, args) => owner.scheduleCallSmall(
      resolve(name).id, args.length, args[0] ?? 0, args[1] ?? 0, args[2] ?? 0, args[3] ?? 0, 100,
    );
    small(resolve('InitializeCriticalSection'), [pointer]);
    assert.equal(scheduled('EnterCriticalSection', [pointer]), 0, 'uncontended lock returns a scalar');
    owner.setThread(2);
    const wait = JSON.parse(scheduled('EnterCriticalSection', [pointer]));
    assert.equal(wait.kind, 'wait', 'contended lock retains its wait descriptor');
    owner.setThread(1); small(resolve('LeaveCriticalSection'), [pointer]);
    assert.equal(JSON.parse(owner.takeReadyWaits())[0].token, wait.token);
    owner.setThread(2); small(resolve('LeaveCriticalSection'), [pointer]);
    assert.deepEqual(JSON.parse(scheduled('ExitThread', [99])), { kind: 'exit', code: 99 });
    assert.throws(() => scheduled('EnterCriticalSection', []), /invalid scheduled API/);
    assert.throws(() => scheduled('GetVersion', []), /invalid scheduled API/);
    assert.throws(() => owner.scheduleCallSmall(0, 5, 0, 0, 0, 0, 100), /at most four/);
  } finally { owner.free(); }

  const { ApiDispatcher } = await import(new URL('./dispatch.js', directory));
  const registers = Object.fromEntries(['esp', 'eax', 'actual_return'].map(name => [name, new WebAssembly.Global({ value: 'i32', mutable: true }, 0)]));
  let selected, selections = 0, scalarCalls = 0, vectorCalls = 0, values;
  const process = {
    setThread(id) { selected = id; selections++; },
    callSmall(id, count, ...args) { assert.equal(selected, 7); scalarCalls++; values = args.slice(0, count); return values[0] + 1; },
    call(id, args) { vectorCalls++; return args[4]; },
  };
  const host = new ApiDispatcher({ process, memory, registers }, { yields: 0 }, () => {}, () => {});
  host.threadId = 7; host.threads = { signal: new AbortController().signal, notify() {} };
  const binding = { label: 'test!small', api: { arity: 1, id: 1, stackArgs: 1, dll: 'test' } };
  const stack = args => {
    registers.esp.value = 0x1000;
    const view = new DataView(memory.buffer);
    [0x12345678, ...args].forEach((value, i) => view.setUint32(0x1000 + 4 * i, value, true));
  };
  stack([41]); host.callImport(binding);
  assert.equal(registers.eax.value, 42); assert.equal(registers.esp.value, 0x1008);
  assert.equal(selections, 1, 'synchronous completion retains the selected thread');
  assert.equal(scalarCalls, 1); assert.deepEqual(values, [41]);
  stack([1, 2, 3, 4, 5]);
  host.callImport({ ...binding, api: { ...binding.api, arity: 5, stackArgs: 5 } });
  assert.equal(registers.eax.value, 5); assert.equal(vectorCalls, 1);
  let resume;
  host.threads.schedule = () => new Promise(done => { resume = done; });
  stack([77]); const pending = host.callImport({ ...binding, api: { ...binding.api, schedule: true } });
  selected = 9; resume(8); await pending;
  assert.equal(selected, 7, 'a suspended call restores its thread before completion');
  assert.equal(registers.eax.value, 8); assert.equal(scalarCalls, 1, 'waits use the scheduled API');
  process.callSmall = () => { throw new Error('scalar failure'); };
  stack([88]); assert.throws(() => host.callImport(binding), /scalar failure; args=0x58.*caller 0x12345678/);
  host.bindings = [{ ...binding, api: { ...binding.api, schedule: true } }];
  assert.throws(() => host.dispatchSync(16), /requires suspending/);
  assert.equal(registers.esp.value, 0x1000, 'a misclassified async call has no guest side effects');
  host.bindings = [binding]; host.callImport = () => Promise.resolve();
  assert.throws(() => host.dispatchSync(16), /pending operation/);
  host.callImport = ApiDispatcher.prototype.callImport;
  host.bindings = [{ ...binding, api: { ...binding.api, schedule: true } }];
  host.threads.schedule = () => 23;
  const beforeImmediate = selections;
  stack([3]); assert.equal(host.dispatchSchedule(16), 0);
  assert.equal(registers.eax.value, 23);
  assert.equal(selections - beforeImmediate, 1, 'immediate scheduling retains its selected thread');
  assert.throws(() => host.awaitSchedule(), /No scheduled operation/);
  host.threads.schedule = () => new Promise(done => { resume = done; });
  stack([4]); assert.equal(host.dispatchSchedule(16), 1);
  assert.throws(() => host.dispatchSchedule(16), /already pending/);
  const scheduled = host.awaitSchedule();
  selected = 9; resume(33); await scheduled;
  assert.equal(registers.eax.value, 33); assert.equal(selected, 7);
  assert.equal(host.pendingSchedule, null);
  host.bindings = [binding];
  assert.throws(() => host.dispatchSchedule(16), /different dispatch path/);

  let fusedCalls = 0, fusedValues;
  process.callSmallForThread = (thread, id, count, ...args) => {
    selected = thread;
    fusedCalls++;
    fusedValues = args.slice(0, count);
    return 0xffffffff;
  };
  const fast = new ApiDispatcher({ process, memory, registers }, { yields: 0 }, () => {}, () => {});
  fast.threadId = 7; fast.threads = host.threads;
  for (let arity = 0; arity <= 4; arity++) {
    fast.bindings = [{ ...binding, api: { ...binding.api, arity, stackArgs: arity } }];
    stack([11, 22, 33, 44]); selected = 9;
    const before = selections;
    fast.dispatchSync(16);
    assert.equal(selected, 7, 'fused dispatch selects its owner despite external thread switches');
    assert.equal(selections, before, 'fused dispatch avoids the separate selection export');
    assert.equal(registers.eax.value >>> 0, 0xffffffff);
    assert.equal(registers.actual_return.value, 0x12345678);
    assert.equal(registers.esp.value, 0x1004 + arity * 4);
    assert.deepEqual(fusedValues, [11, 22, 33, 44].slice(0, arity));
  }
  memory.grow(1);
  stack([88]); fast.bindings = [binding];
  fast.dispatchSync(16);
  assert.deepEqual(fusedValues, [88], 'fused stack views refresh after memory growth');
  const beforeBadStack = fusedCalls;
  registers.esp.value = memory.buffer.byteLength - 2; selected = 9;
  assert.throws(() => fast.dispatchSync(16), RangeError);
  assert.equal(selected, 7, 'invalid stacks retain the prior thread-selection behavior');
  assert.equal(fusedCalls, beforeBadStack, 'an invalid stack never enters the kernel call');
  process.callSmallForThread = () => { throw new Error('fused failure'); };
  stack([88]);
  assert.throws(() => fast.dispatchSync(16), /fused failure; args=0x58.*caller 0x12345678/);
  let fallback = 0;
  fast.callImport = () => { fallback++; };
  for (const api of [{ ...binding.api, arity: 5 }, { ...binding.api, variadic: true }]) {
    fast.bindings = [{ ...binding, api }];
    fast.dispatchSync(16);
  }
  assert.equal(fallback, 2, 'large and variadic APIs keep the established dispatch path');
  let statusCalls = 0, consumed = 0;
  process.callSmallForThreadStatus = thread => { selected = thread; statusCalls++; return -1; };
  process.takeSyncCallError = () => { consumed++; return new Error('status failure'); };
  const statusHost = new ApiDispatcher({ process, memory, registers }, { yields: 0 }, () => {}, () => {});
  const { ThreadHost } = await import(new URL('./threads.js', directory));
  const guarded = new ThreadHost({});
  statusHost.threadId = 7; statusHost.threads = guarded;
  statusHost.checkAbort = guarded.checkAbort.bind(guarded);
  statusHost.bindings = [binding]; stack([88]);
  assert.throws(() => statusHost.dispatchSync(16), /status failure; args=0x58.*caller 0x12345678/);
  assert.equal(consumed, 1, 'the JS status failure consumes exactly one per-process error');
  assert.equal(statusCalls, 1);
  assert.equal(registers.esp.value, 0x1000, 'failed status calls do not complete the guest stack');
  const stopped = new Error('stopped before API');
  guarded.cancellation.abort(stopped); selected = 9;
  assert.throws(() => statusHost.dispatchSync(16), error => error === stopped);
  assert.equal(statusCalls, 1, 'abort prevents a kernel call or error-slot mutation');
  assert.equal(selected, 9, 'abort prevents thread selection too');
  await guarded.close();
  console.log('Scalar bridge: API parity, arity/clock/memory errors, synchronous thread selection and suspended completion passed');
}
