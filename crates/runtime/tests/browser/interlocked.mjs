import assert from 'node:assert/strict';

export function memoryUpdatesSmoke(updateU32) {
  for (const shared of [false, true]) {
    const memory = new WebAssembly.Memory({ initial: 1, maximum: 2, shared });
    assert.equal(updateU32(memory, 65532, 1, 0xffffffff, 0), 0);
    assert.equal(updateU32(memory, 65532, 0, 1, 0), 0xffffffff);
    assert.equal(updateU32(memory, 65533, 1, 9, 0), -1, 'partial DWORD is out of bounds');
    assert.equal(new DataView(memory.buffer).getUint32(65532, true), 0);
    memory.grow(1);
    assert.equal(updateU32(memory, 65536, 0, 1, 0), 0);
    assert.equal(new DataView(memory.buffer).getUint32(65536, true), 1);
  }
}

// Exercise the real Rust/Wasm API and its browser memory bridge.
export function interlockedSmoke(Process) {
  const memory = new WebAssembly.Memory({ initial: 4, maximum: 32 });
  const owner = new Process(memory, 0x10000, 0x200000);
  try {
    const api = name => JSON.parse(owner.resolve('kernel32.dll', name));
    const call = (name, args) => owner.callSmall(api(name).id, args.length,
      args[0] ?? 0, args[1] ?? 0, args[2] ?? 0, args[3] ?? 0);
    const pointer = owner.heapAlloc(16);
    const view = () => new DataView(memory.buffer);
    const write = (address, value) => view().setUint32(address, value, true);
    const read = address => view().getUint32(address, true);
    const operations = [
      ['InterlockedIncrement', (old, value, compare) => [old + 1, old + 1], []],
      ['InterlockedDecrement', (old, value, compare) => [old - 1, old - 1], []],
      ['InterlockedExchange', (old, value, compare) => [old, value], [0]],
      ['InterlockedExchangeAdd', (old, value, compare) => [old, old + value], [0]],
      ['InterlockedCompareExchange', (old, value, compare) => [old, old === compare ? value : old], [0, 0]],
    ];
    call('SetLastError', [0x91234567]);
    const values = [0, 1, 0xffffffff, 0x7fffffff, 0x80000000, 0x12345678];
    for (const [name, expected, placeholders] of operations) {
      for (const old of values) for (const value of values) for (const compare of [old, (old + 1) >>> 0]) {
        write(pointer, old);
        const args = [pointer, ...[value, compare].slice(0, placeholders.length)];
        const [returned, updated] = expected(old, value, compare);
        assert.equal(call(name, args), returned >>> 0, name);
        assert.equal(read(pointer), updated >>> 0, name);
      }
    }
    assert.equal(call('GetLastError', []), 0x91234567);

    owner.map(0x2000, Uint8Array.of(0x78, 0x56, 0x34, 0x12), false);
    owner.reserve(0x3000, 4);
    owner.map(0x4000, new Uint8Array(3), true);
    owner.map(0x6000, new Uint8Array(2), false);
    // Adjacent owned mappings may jointly contain the DWORD.
    owner.map(0x5000, Uint8Array.of(0xff, 0xff), true);
    owner.map(0x5002, Uint8Array.of(0xff, 0xff), true);
    assert.equal(call('InterlockedIncrement', [0x5000]), 0);
    assert.equal(read(0x5000), 0);
    for (const address of [0, pointer + 1, 0x2000, 0x3000, 0x4000, 0x6000, 0xfffffffc]) {
      const before = new Uint8Array(memory.buffer).slice();
      for (const [name, , placeholders] of operations) {
        // Compare-exchange does not match the read-only value, but still requires write access.
        assert.throws(() => call(name, [address, ...[7, 9].slice(0, placeholders.length)]));
        assert.deepEqual(new Uint8Array(memory.buffer), before, name + ' invalid write');
      }
    }
    assert.throws(() => call('InterlockedIncrement', [0x6000]), /not mapped/,
      'an incomplete read fails before checking write protection');

    // Growth detaches the cached DataView; a later update must use the new buffer.
    write(pointer, 0xffffffff);
    memory.grow(1);
    assert.equal(call('InterlockedIncrement', [pointer]), 0);
    assert.equal(read(pointer), 0);
    const virtual = owner.virtualAlloc(0x80000, 0x1000);
    write(virtual, 41);
    assert.equal(call('InterlockedIncrement', [virtual]), 42);
    assert.equal(call('VirtualFree', [virtual, 0x1000, 0x4000]), 1);
    assert.throws(() => call('InterlockedExchange', [virtual, 99]), /mapped/);
    assert.equal(read(virtual), 0, 'decommitted backing stays untouched');
    assert.equal(call('VirtualAlloc', [virtual, 0x1000, 0x1000, 4]), virtual);
    assert.equal(call('InterlockedIncrement', [virtual]), 1);
    owner.virtualFree(virtual);
    assert.throws(() => call('InterlockedCompareExchange', [virtual, 2, 9]), /mapped/);
    assert.equal(read(virtual), 0, 'released backing stays untouched');
    const reserved = call('VirtualAlloc', [0xa0000, 0x1000, 0x2000, 1]);
    assert.equal(reserved, 0xa0000);
    assert.throws(() => call('InterlockedIncrement', [reserved]), /mapped/);
    console.log('Interlocked bridge: wrapping, compare failures, ownership/protection, growth, decommit and release passed');
  } finally { owner.free(); }
}
