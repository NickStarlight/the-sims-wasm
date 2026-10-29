import assert from 'node:assert/strict';

export async function optimizationSmoke(Process, directory) {
  const { yieldToHost } = await import(new URL('./scheduler.js', directory));
  const { schedulerSmoke } = await import('./scheduler.mjs');
  await schedulerSmoke(yieldToHost);
  const { createFileReaders } = await import(new URL('./file-source.js', directory));
  const reads = [];
  const file = bytes => ({ size: bytes.length, slice(start, end) { reads.push([start, end]); return bytes.slice(start, end); } });
  const register = createFileReaders(bytes => bytes.buffer, 8, 4);
  const read = register(file(Uint8Array.from({ length: 17 }, (_, i) => i)));
  assert.deepEqual([...read(3, 7)], [3, 4, 5, 6, 7, 8, 9]);
  const count = reads.length;
  read(4, 4); assert.equal(reads.length, count, 'page reuse');
  read(12, 4); read(8, 1); assert.equal(reads.length, count + 2, 'least recently used page was evicted');
  assert.deepEqual([...read(16, 1)], [16]);
  const second = register(file(Uint8Array.of(99, 98, 97, 96)));
  assert.deepEqual([...second(0, 2)], [99, 98], 'file cache keys must not alias');
  assert.throws(() => read(-1, 1), RangeError);
  assert.throws(() => read(16, 2), RangeError);
  assert.deepEqual([...read(17, 0)], []);

  const persistentPages = new Map();
  const stats = {};
  let sourceReads = 0;
  const cachedRegister = createFileReaders(bytes => { sourceReads++; return bytes.buffer; }, 4, 4, {
    stats,
    readPage(file, path, start, length, original) {
      const key = `${path}:${start}`;
      if (!persistentPages.has(key)) persistentPages.set(key, original(start, length));
      return persistentPages.get(key);
    },
  });
  const cachedRead = cachedRegister(file(Uint8Array.from({ length: 9 }, (_, i) => i)), 'GameData/fonts.bin');
  assert.deepEqual([...cachedRead(2, 7)], [2, 3, 4, 5, 6, 7, 8]);
  assert.equal(sourceReads, 3);
  assert.deepEqual([...cachedRead(0, 4)], [0, 1, 2, 3]);
  assert.equal(sourceReads, 3, 'persistent pages survive RAM LRU eviction');
  assert.equal(stats.sourceBytes, 9);
  assert.equal(stats.memoryMisses, 4);

  const memory = new WebAssembly.Memory({ initial: 4, maximum: 32 });
  const owner = new Process(memory, 0x10000, 0x200000);
  try {
    let loads = 0;
    owner.addFileReader('deferred.bin', 4, (offset, length) => {
      loads++; return Uint8Array.of(9, 8, 7, 6).slice(offset, offset + length);
    });
    assert.equal(loads, 0);
    assert.throws(() => owner.addFileReader('bad', NaN, () => new Uint8Array()));
    const invoke = (name, args) => owner.call(JSON.parse(owner.resolve('kernel32.dll', name)).id, Uint32Array.from(args));
    const scratch = owner.heapAlloc(128);
    owner.write(scratch, new TextEncoder().encode('deferred.bin\0'));
    const handle = invoke('CreateFileA', [scratch, 0x80000000, 3, 0, 3, 0, 0]);
    assert.notEqual(handle, 0xffffffff);
    assert.equal(invoke('GetFileSize', [handle, 0]), 4);
    assert.equal(loads, 0, 'open and size must not read data');
    assert.equal(invoke('ReadFile', [handle, scratch + 32, 2, scratch + 64, 0]), 1);
    assert.deepEqual([...owner.read(scratch + 32, 2)], [9, 8]);
    assert.equal(loads, 1);
  } finally { owner.free(); }

  const { ApiDispatcher } = await import(new URL('./dispatch.js', directory));
  const registers = Object.fromEntries(['esp', 'eax', 'actual_return'].map(name => [name, new WebAssembly.Global({ value: 'i32', mutable: true }, 0)]));
  let selected = 0, seen;
  const process = { setThread(id) { selected = id; }, call(id, args) { assert.equal(selected, 7); seen = args; return args[0] + 1; } };
  const host = new ApiDispatcher({ process, memory, registers }, { yields: 0 }, () => {}, () => {});
  host.threadId = 7;
  host.threads = { signal: new AbortController().signal, drain() {} };
  const binding = { label: 'test!sync', api: { arity: 1, id: 1, stackArgs: 1, dll: 'test' } };
  const stack = value => {
    registers.esp.value = 0x1000;
    const view = new DataView(memory.buffer);
    view.setUint32(0x1000, 0x12345678, true);
    view.setUint32(0x1004, value, true);
  };
  stack(41); host.callImport(binding);
  const reusable = seen;
  assert.equal(registers.eax.value, 42);
  assert.equal(registers.esp.value, 0x1008);
  assert.equal(registers.actual_return.value, 0x12345678);
  memory.grow(1);
  stack(99); host.callImport(binding);
  assert.equal(registers.eax.value, 100);
  assert.equal(seen, reusable, 'ordinary synchronous calls reuse arguments');
  let held, resolve;
  host.threads.schedule = (id, args) => { held = args; return new Promise(done => { resolve = done; }); };
  stack(17);
  const waiting = host.callImport({ ...binding, api: { ...binding.api, schedule: true } });
  stack(23); host.callImport(binding);
  assert.equal(held[0], 17);
  resolve(7); await waiting;
  assert.equal(registers.eax.value, 7);
  process.beginCall = (id, args) => { held = args; return JSON.stringify({ kind: 'callback', token: 1, address: 2, args: [] }); };
  process.resumeCall = () => JSON.stringify({ kind: 'return', value: 8 });
  process.cancelCall = () => {};
  host.callGuest = async () => { stack(91); host.callImport(binding); return 0; };
  stack(31);
  await host.callImport({ ...binding, api: { ...binding.api, callback: true } });
  assert.equal(held[0], 31);
  assert.equal(registers.eax.value, 8);
  let reject;
  host.threads.schedule = () => new Promise((done, fail) => { reject = fail; });
  stack(77);
  const rejected = host.callImport({ ...binding, api: { ...binding.api, schedule: true } });
  stack(88); host.callImport(binding);
  reject(new Error('scheduled failure'));
  await assert.rejects(rejected, /args=0x4d/);
  console.log('Optimization regressions: scheduler fairness, bounded file cache, deferred Wasm files, dispatcher memory growth and reentry passed');
}
