// Actual generated runtime + direct multi-memory helper regression.
// node guest-memory.mjs /path/to/runtime.js [--fallback] [--four-gib]
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { pathToFileURL } from 'node:url';
import { interlockedSmoke } from './interlocked.mjs';

const moduleUrl = pathToFileURL(resolve(process.argv[2]));
const link = await import(new URL('./guest-memory.js', moduleUrl));
const fallback = process.argv.includes('--fallback');
if (fallback) {
  // Cache a failed feature probe for each memory signature, without a production
  // feature flag or replacing the actual runtime/helper imports.
  const validate = WebAssembly.validate;
  try {
    WebAssembly.validate = () => false;
    for (const shared of [false, true]) {
      assert.equal(link.registerGuestMemory(new WebAssembly.Memory({ initial: 1, maximum: 2, shared }),
        new WebAssembly.Memory({ initial: 1 })), -1);
    }
  } finally { WebAssembly.validate = validate; }
}
const { default: init, Process } = await import(moduleUrl);
const runtime = await init({ module_or_path: readFileSync(new URL('./runtime_bg.wasm', moduleUrl)) });
const table = new WebAssembly.Table({ element: 'anyfunc', initial: 1 });
table.set(0, link.guestMemoryAccess);
assert.equal(table.get(0), link.guestMemoryAccess, 'the raw import is an actual Wasm function');
assert.throws(() => new Process(runtime.memory, 0x10000, 0x100000), /must be distinct/);

if (!fallback) {
  const vm = await import('node:vm');
  const foreign = vm.runInNewContext('new WebAssembly.Memory({initial: 1, maximum: 2, shared: true})');
  const localShared = new WebAssembly.Memory({ initial: 1, maximum: 2, shared: true });
  const descriptor = Object.getOwnPropertyDescriptor(globalThis, 'SharedArrayBuffer');
  try {
    Object.defineProperty(globalThis, 'SharedArrayBuffer', { configurable: true, value: undefined });
    for (const memory of [foreign, localShared]) {
      const handle = link.registerGuestMemory(memory, runtime.memory);
      assert.ok(handle >= 0, 'shared buffer detection works across realms and with a hidden constructor');
      assert.equal(link.guestMemoryAccess(handle, 1, 0, 4, 0xabcdef01, 0), 0);
      assert.equal(new DataView(memory.buffer).getUint32(0, true), 0xabcdef01);
      link.releaseGuestMemory(handle);
    }
  } finally { Object.defineProperty(globalThis, 'SharedArrayBuffer', descriptor); }
  for (const kind of ['unsupported', 'compile', 'link', 'unexpected']) {
    const fresh = await import(new URL(`./guest-memory.js?test=${kind}`, moduleUrl));
    const guest = new WebAssembly.Memory({ initial: 1, maximum: 2 });
    const scratch = new WebAssembly.Memory({ initial: 1 });
    const previous = { validate: WebAssembly.validate, Module: WebAssembly.Module, Instance: WebAssembly.Instance };
    try {
      if (kind === 'unsupported') WebAssembly.validate = () => false;
      if (kind === 'compile') WebAssembly.Module = function () { throw new WebAssembly.CompileError('test'); };
      if (kind === 'link') WebAssembly.Instance = function () { throw new WebAssembly.LinkError('test'); };
      if (kind === 'unexpected') WebAssembly.Instance = function () { throw new Error('unexpected host failure'); };
      assert.throws(() => fresh.registerGuestMemory(scratch, scratch), /must be distinct/);
      if (kind === 'unexpected') assert.throws(() => fresh.registerGuestMemory(guest, scratch), /unexpected host failure/);
      else assert.equal(fresh.registerGuestMemory(guest, scratch), -1);
    } finally { Object.assign(WebAssembly, previous); }
    if (kind === 'unsupported') assert.equal(fresh.registerGuestMemory(guest, scratch), -1, 'unsupported probe is cached');
    else {
      const handle = fresh.registerGuestMemory(guest, scratch);
      assert.equal(handle, 0, 'cold failure does not consume or retain a table slot');
      fresh.releaseGuestMemory(handle);
    }
  }
  for (const shared of [false, true]) {
    const guest = new WebAssembly.Memory({ initial: 2, maximum: 4, shared });
    const scratch = new WebAssembly.Memory({ initial: 2, maximum: 4 });
    const handle = link.registerGuestMemory(guest, scratch);
    assert.ok(handle >= 0);
    const call = (op, a = 0, b = 0, c = 0, d = 0) => link.guestMemoryAccess(handle, op, a, b, c, d);
    try {
      for (const width of [1, 2, 4]) for (const address of [0, 1, 3, 127, 131072 - width]) {
        for (const value of [0, 1, 0x7fffffff, 0x80000000, 0xffffffff]) {
          assert.equal(call(1, address, width, value), 0);
          assert.equal(call(0, address, width), width === 4 ? value : value & ((1 << (width * 8)) - 1));
        }
      }
      for (const [address, width] of [[131071, 2], [131070, 4], [0xffffffff, 4]]) {
        const before = new Uint8Array(guest.buffer).slice();
        assert.equal(call(0, address, width), -1);
        assert.equal(call(1, address, width, 0), -1);
        assert.deepEqual(new Uint8Array(guest.buffer), before);
      }
      for (const size of [0, 1, 2, 3, 4, 7, 63, 257, 65536]) {
        const source = Uint8Array.from({ length: size }, (_, i) => (i * 83) & 255);
        new Uint8Array(guest.buffer, 1, size).set(source);
        assert.equal(call(2, 1, 3, size), 0);
        assert.deepEqual(new Uint8Array(scratch.buffer, 3, size), source);
        assert.equal(call(3, 7, 3, size), 0);
        assert.deepEqual(new Uint8Array(guest.buffer, 7, size), source);
      }
      for (const [op, a, b, size] of [[2, 131071, 0, 2], [2, 0, 131071, 2], [3, 131071, 0, 2], [3, 0, 131071, 2]]) {
        const beforeGuest = new Uint8Array(guest.buffer).slice();
        const beforeScratch = new Uint8Array(scratch.buffer).slice();
        assert.equal(call(op, a, b, size), -1);
        assert.deepEqual(new Uint8Array(guest.buffer), beforeGuest);
        assert.deepEqual(new Uint8Array(scratch.buffer), beforeScratch);
      }
      for (const [destination, source, size] of [[0, 1, 127], [1, 0, 127], [3, 3, 257], [500, 1, 257]]) {
        const expected = Uint8Array.from({ length: 131072 }, (_, i) => i & 255);
        new Uint8Array(guest.buffer).set(expected);
        expected.copyWithin(destination, source, source + size);
        assert.equal(call(5, destination, source, size), 0);
        assert.deepEqual(new Uint8Array(guest.buffer), expected);
      }
      assert.equal(call(5, 0xffffffff, 0xffffffff, 0), 0);
      for (const address of [0, 1, 131068]) for (const previous of [0, 1, 0x7fffffff, 0x80000000, 0xffffffff]) {
        for (const kind of [0, 1, 2]) for (const compare of [previous, (previous + 1) >>> 0]) {
          call(1, address, 4, previous);
          assert.equal(call(6, address, kind, 0xffffffff, compare), previous);
          const expected = kind === 0 ? (previous - 1) >>> 0 : kind === 1 || previous === compare ? 0xffffffff : previous;
          assert.equal(call(0, address, 4), expected);
        }
      }
      assert.equal(call(4, 11, 257, 0x1aa), 0);
      assert.deepEqual(new Uint8Array(guest.buffer, 11, 257), new Uint8Array(257).fill(0xaa));
      assert.equal(call(4, 131071, 2, 0), -1);
      assert.equal(call(4, 131072, 0, 0), 0);
      assert.equal(call(4, 131073, 0, 0), -1);
      assert.equal(call(7), 131072);
      guest.grow(1);
      scratch.grow(1);
      assert.equal(call(7), 196608);
      assert.equal(call(1, 131073, 4, 0xdeadbeef), 0);
      assert.equal(call(2, 131073, 131075, 4), 0);
      assert.equal(new DataView(scratch.buffer).getUint32(131075, true), 0xdeadbeef);
    } finally { link.releaseGuestMemory(handle); }
    assert.throws(() => call(0, 0, 4), WebAssembly.RuntimeError);
    const reused = link.registerGuestMemory(guest, scratch);
    assert.equal(reused, handle, 'released table slots are reused');
    link.releaseGuestMemory(reused);
    link.releaseGuestMemory(reused); // A repeated release must not duplicate a free slot.
    assert.throws(() => new Process(guest, 0x20000, 0x10000));
    const afterFailure = link.registerGuestMemory(guest, scratch);
    assert.equal(afterFailure, handle, 'failed Rust constructor releases its registered slot');
    const concurrent = link.registerGuestMemory(guest, scratch);
    assert.notEqual(concurrent, afterFailure);
    link.releaseGuestMemory(concurrent);
    link.releaseGuestMemory(afterFailure);
  }
}
for (const shared of [false, true]) {
  const firstMemory = new WebAssembly.Memory({ initial: 2, maximum: 128, shared });
  const secondMemory = new WebAssembly.Memory({ initial: 2, maximum: 128, shared });
  const first = new Process(firstMemory, 0x10000, 0x800000);
  const second = new Process(secondMemory, 0x10000, 0x800000);
  try {
    first.map(0x1000, Uint8Array.of(1, 2, 3, 4), true);
    second.map(0x1000, Uint8Array.of(9, 8, 7, 6), true);
    for (const width of [1, 2, 3, 4, 7, 63, 257, 65536]) {
      const address = first.heapAlloc(width + 2) + 1;
      const expected = Uint8Array.from({ length: width }, (_, i) => i * 3 & 255);
      first.write(address, expected);
      assert.deepEqual(first.read(address, width), expected);
    }
    assert.deepEqual(first.read(0x1000, 4), Uint8Array.of(1, 2, 3, 4));
    assert.deepEqual(second.read(0x1000, 4), Uint8Array.of(9, 8, 7, 6));
    firstMemory.grow(1);
    first.write(0x1001, Uint8Array.of(0xaa, 0xbb));
    assert.deepEqual(first.read(0x1000, 4), Uint8Array.of(1, 0xaa, 0xbb, 4));
    first.map(0x4000, new Uint8Array(7), false);
    assert.throws(() => first.write(0x4000, new Uint8Array(7)), /protected/);
    assert.throws(() => first.write(0x4006, new Uint8Array(2)), /protected/);
    const large = new Uint8Array(4 * 1024 * 1024).fill(0x93);
    const address = second.heapAlloc(large.length);
    second.write(address, large);
    assert.deepEqual(second.read(address, large.length), large);
    assert.deepEqual(first.read(0x1000, 4), Uint8Array.of(1, 0xaa, 0xbb, 4));
  } finally { first.free(); second.free(); }
}
interlockedSmoke(Process);
if (process.argv.includes('--four-gib')) {
  for (const shared of [false, true]) {
    const memory = new WebAssembly.Memory({ initial: 65536, maximum: 65536, shared });
    const owner = new Process(memory, 0x10000, 2 ** 32);
    try {
      owner.map(0xfffffff0, new Uint8Array(16), true);
      owner.write(0xffffffff, Uint8Array.of(0xab));
      assert.deepEqual(owner.read(0xffffffff, 1), Uint8Array.of(0xab));
      owner.write(0xfffffffc, Uint8Array.of(0xff, 0xff, 0xff, 0xff));
      const increment = JSON.parse(owner.resolve('kernel32.dll', 'InterlockedIncrement')).id;
      assert.equal(owner.callSmall(increment, 1, 0xfffffffc, 0, 0, 0), 0);
      assert.deepEqual(owner.read(0xfffffffc, 4), new Uint8Array(4));
      owner.write(0xfffffff9, Uint8Array.of(1, 2, 3, 4, 5, 6, 7));
      assert.deepEqual(owner.read(0xfffffff9, 7), Uint8Array.of(1, 2, 3, 4, 5, 6, 7));
      assert.throws(() => owner.write(0xffffffff, Uint8Array.of(1, 2)));
      assert.deepEqual(owner.read(0xffffffff, 1), Uint8Array.of(7));
    } finally { owner.free(); }
  }
}
console.log(`Guest memory linkage: ${fallback ? 'forced DataView fallback' : 'direct Wasm'}, scalar/bulk/RMW, bounds, lifecycle, isolation and growth passed${process.argv.includes('--four-gib') ? ', including 4 GiB' : ''}`);
