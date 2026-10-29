import assert from 'node:assert/strict';

// Exercise generated Wasm exports, including the state/error ordering of the
// original two-export sequence. Each process has independent guest memory.
export function threadBridgeSmoke(Process) {
  for (const status of [false, true]) checkThreadBridge(Process, status);
}

function checkThreadBridge(Process, status) {
  const create = () => new Process(new WebAssembly.Memory({ initial: 4, maximum: 32 }), 0x10000, 0x200000);
  const original = create();
  const fused = create();
  try {
    const resolve = name => JSON.parse(original.resolve('kernel32.dll', name)).id;
    const currentThread = resolve('GetCurrentThreadId');
    const pointer = original.heapAlloc(32);
    assert.equal(fused.heapAlloc(32), pointer);
    const outcome = operation => {
      try { return { value: operation() }; }
      catch (error) { return { error: error.message }; }
    };
    const compare = (thread, id, args = [], now, wall, count = args.length) => {
      const values = [id, count, args[0] ?? 0, args[1] ?? 0, args[2] ?? 0, args[3] ?? 0, now, wall];
      const expected = outcome(() => {
        original.setThread(thread);
        return original.callSmall(...values);
      });
      const actual = outcome(() => {
        if (!status) return fused.callSmallForThread(thread, ...values);
        const value = fused.callSmallForThreadStatus(thread, ...values);
        if (value < 0) throw fused.takeSyncCallError();
        assert.equal(fused.takeSyncCallError(), undefined, 'successful calls leave no pending transport error');
        return value;
      });
      assert.deepEqual(actual, expected, `thread${thread} API${id} count${count}`);
      assert.equal(fused.callSmall(currentThread, 0, 0, 0, 0, 0), original.callSmall(currentThread, 0, 0, 0, 0, 0), 'selection persists after both success and failure');
      assert.deepEqual([...fused.read(pointer, 32)], [...original.read(pointer, 32)]);
      return actual;
    };
    const call = (thread, name, args = [], now, wall) => {
      const result = compare(thread, resolve(name), args, now, wall);
      assert.equal(result.error, undefined, name);
      return result.value;
    };
    const fails = (thread, name, args, pattern, now, wall, count) => {
      const result = compare(thread, typeof name === 'number' ? name : resolve(name), args, now, wall, count);
      assert.match(result.error ?? '', pattern);
    };

    const slot = call(1, 'TlsAlloc');
    for (const [thread, value] of [[1, 0x11111111], [2, 0xeeeeeeee]]) {
      assert.equal(call(thread, 'TlsSetValue', [slot, value]), 1);
      call(thread, 'SetLastError', [value]);
    }
    for (const thread of [2, 1, 2, 1]) {
      const value = thread === 1 ? 0x11111111 : 0xeeeeeeee;
      assert.equal(call(thread, 'GetCurrentThreadId'), thread);
      assert.equal(call(thread, 'GetLastError'), value);
    }
    assert.equal(call(2, 'TlsGetValue', [slot]), 0xeeeeeeee);
    assert.equal(call(2, 'GetLastError'), 0, 'TLS read clears only the selected thread error');
    assert.equal(call(1, 'GetLastError'), 0x11111111);
    assert.equal(call(1, 'TlsGetValue', [slot]), 0x11111111);
    assert.equal(call(2, 'TlsSetValue', [64, 7]), 0);
    assert.equal(call(2, 'GetLastError'), 87);
    call(1, 'InterlockedExchange', [pointer, 0xffffffff]);
    assert.equal(call(2, 'InterlockedExchange', [pointer, 123]), 0xffffffff, 'all DWORD results remain distinct from transport failure');
    assert.equal(call(1, 'GetLastError'), 0);

    call(1, 'InitializeCriticalSection', [pointer]);
    assert.equal(call(1, 'TryEnterCriticalSection', [pointer]), 1);
    assert.equal(call(2, 'TryEnterCriticalSection', [pointer]), 0);
    fails(2, 'LeaveCriticalSection', [pointer], /another thread/);
    call(1, 'LeaveCriticalSection', [pointer]);
    assert.equal(call(2, 'TryEnterCriticalSection', [pointer]), 1);
    call(2, 'LeaveCriticalSection', [pointer]);

    fails(0, 0xffffffff, [], /invalid guest address range/, undefined, undefined, 5);
    fails(1, 'GetVersion', [1], /expects 0 arguments/);
    fails(2, 0xffffffff, [], /at most four/, undefined, undefined, 5);
    fails(1, 0xffffffff, [], /unknown kernel API/);
    fails(2, 'InterlockedIncrement', [0], /mapped|protected|range/i);
    fails(1, 'GetVersion', [], /non-clock API/, 1);
    fails(2, 'GetTickCount', [], /fresh monotonic/);
    fails(1, 'GetTickCount', [], /invalid or decreasing monotonic/, Number.NaN);
    assert.equal(call(2, 'GetTickCount', [], 123.5), 123);
    fails(1, 'GetTickCount', [7], /expects 0 arguments/, 200);
    fails(2, 'GetTickCount', [], /invalid or decreasing monotonic/, 199);
    assert.equal(call(1, 'GetTickCount', [], 201), 201);
    fails(2, 'GetSystemTimeAsFileTime', [pointer], /fresh sample/);
    fails(1, 'GetSystemTimeAsFileTime', [pointer], /invalid guest address range/, undefined, Infinity);
    call(2, 'GetSystemTimeAsFileTime', [pointer], undefined, 1700000000000);
    assert.equal(new DataView(fused.read(pointer, 8).buffer).getBigUint64(0, true), 133444736000000000n);
    assert.equal(call(1, 'TlsFree', [slot]), 1);
    assert.equal(call(2, 'TlsGetValue', [slot]), 0);
    assert.equal(call(2, 'GetLastError'), 87);
    if (status) {
      assert.equal(fused.callSmallForThreadStatus(1, 0xffffffff, 0, 0, 0, 0, 0), -1);
      const error = fused.takeSyncCallError();
      assert.match(error.message, /unknown kernel API/);
      assert.equal(fused.takeSyncCallError(), undefined, 'transport errors are consumed once');
      assert.equal(fused.callSmallForThreadStatus(1, 0xffffffff, 0, 0, 0, 0, 0), -1);
      assert.equal(fused.callSmallForThreadStatus(1, currentThread, 0, 0, 0, 0, 0), 1);
      assert.equal(fused.takeSyncCallError(), undefined, 'the next successful call clears an unconsumed error');
      assert.equal(original.callSmallForThreadStatus(1, currentThread, 5, 0, 0, 0, 0), -1);
      assert.equal(fused.callSmallForThreadStatus(1, 0xffffffff, 0, 0, 0, 0, 0), -1);
      assert.match(original.takeSyncCallError().message, /at most four/);
      assert.match(fused.takeSyncCallError().message, /unknown kernel API/, 'transport errors belong to each process independently');
    }
  } finally {
    original.free();
    fused.free();
  }
  console.log('Fused thread/scalar bridge: alternating TLS/error/lock ownership and validation parity passed');
}
