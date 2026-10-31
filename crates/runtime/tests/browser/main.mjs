// Run after wasm-bindgen --target web: node browser.mjs /path/to/runtime.js
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { pathToFileURL } from 'node:url';

if (!process.argv[2]) throw new Error('Pass the generated runtime.js path');
const moduleUrl = pathToFileURL(resolve(process.argv[2]));
const { default: init, Process } = await import(moduleUrl.href);
const runtime = await init({ module_or_path: readFileSync(new URL('./runtime_bg.wasm', moduleUrl)) });
const guest = new WebAssembly.Memory({ initial: 1, maximum: 2 });
assert.notEqual(runtime.memory, guest, 'Rust state and guest bytes must use different memories');
const first = new Process(guest, 0x10000, 0x40000);
try {
  first.reserve(0, 0x1000);
  first.map(0x1000, Uint8Array.of(1, 2, 3, 4), false);
  assert.throws(() => first.write(0x1000, Uint8Array.of(9)), /protected/);
  assert.throws(() => first.virtualAlloc(0x1000, 4), /invalid guest address range/);
  const address = first.heapAlloc(4);
  first.write(address, Uint8Array.of(4, 3, 2, 1));
  assert.deepEqual([...first.read(address, 4)], [4, 3, 2, 1]);
  assert.equal(guest.buffer.byteLength, 131072, 'guest memory grows without moving Rust state');
  assert.throws(() => first.heapFree(address + 1), /live allocation/);
  assert.throws(() => first.heapRealloc(address, 0x10001));
  assert.equal(first.heapSize(address), 4, 'failed Wasm memory.grow preserves ownership');
  assert.deepEqual([...first.read(address, 4)], [4, 3, 2, 1]);
  first.heapAlloc(16); // A live neighbor forces relocation rather than safe in-place growth.
  const replacement = first.heapRealloc(address, 32);
  assert.deepEqual([...first.read(replacement, 4)], [4, 3, 2, 1]);
  assert.throws(() => first.heapSize(address), /live allocation/);
  assert.equal(first.resolve('missing.dll', 'Missing'), undefined);
  const api = JSON.parse(first.resolve('kernel32.dll', 'GetVersion'));
  assert.equal(api.arity, 0);
  assert.equal(typeof first.call(api.id, new Uint32Array()), 'number');
  assert.throws(() => first.call(api.id, Uint32Array.of(1)), /argument|arity/i);
  assert.throws(() => first.cpuHelper(10, new Int32Array(15), 0), /16 registers/);
  const registers = new Int32Array(16);
  const cpuid = first.cpuHelper(10, registers, 0);
  assert.equal(cpuid.length, 16);
  assert.equal(cpuid[0], 1);
  first.adopt(0x2000, 0x1000, true);
  registers[15] = 0x2000;
  new DataView(guest.buffer).setFloat64(0x2400, Math.PI, true);
  first.cpuHelper(7, registers, 0);
  assert.equal(new DataView(guest.buffer).getFloat64(0x2400, true), -1);
  first.addFile('GameData/Test.far', Uint8Array.of(70, 65, 82, 33, 7, 8));
  first.write(0x2100, new TextEncoder().encode('C:\\Sims\\GameData\\Test.far\0'));
  const invoke = (name, args) => first.call(JSON.parse(first.resolve('kernel32.dll', name)).id, Uint32Array.from(args));
  const handle = invoke('CreateFileA', [0x2100, 0x80000000, 3, 0, 3, 0, 0]);
  assert.notEqual(handle, 0xffffffff);
  assert.equal(invoke('ReadFile', [handle, 0x2300, 4, 0x2380, 0]), 1);
  assert.deepEqual([...first.read(0x2300, 4)], [70, 65, 82, 33]);
  assert.equal(invoke('SetFilePointer', [handle, 4, 0, 0]), 4);
  assert.equal(invoke('ReadFile', [handle, 0x2300, 4, 0x2380, 0]), 1);
  assert.deepEqual([...first.read(0x2300, 2)], [7, 8]);
  assert.equal(new DataView(guest.buffer).getUint32(0x2380, true), 2);
  assert.equal(invoke('CloseHandle', [handle]), 1);
  const registry = (name, args) => first.call(JSON.parse(first.resolve('advapi32.dll', name)).id, Uint32Array.from(args));
  first.write(0x2500, new TextEncoder().encode('Software\\SimsTest\0'));
  assert.equal(registry('RegCreateKeyExA', [0x80000001, 0x2500, 0, 0, 0, 0xf003f, 0, 0x2580, 0x2584]), 0);
  const key = new DataView(guest.buffer).getUint32(0x2580, true);
  assert.equal(new DataView(guest.buffer).getUint32(0x2584, true), 1);
  first.write(0x2600, new TextEncoder().encode('Test\0'));
  first.write(0x2700, Uint8Array.of(9, 8, 7));
  assert.equal(registry('RegSetValueExA', [key, 0x2600, 0, 3, 0x2700, 3]), 0);
  new DataView(guest.buffer).setUint32(0x2780, 2, true);
  first.write(0x27c0, Uint8Array.of(5, 5, 5));
  assert.equal(registry('RegQueryValueExA', [key, 0x2600, 0, 0, 0x27c0, 0x2780]), 234);
  assert.deepEqual([...first.read(0x27c0, 3)], [5, 5, 5]);
  assert.equal(new DataView(guest.buffer).getUint32(0x2780, true), 3);
  assert.equal(registry('RegQueryValueExA', [key, 0x2600, 0, 0, 0x27c0, 0x2780]), 0);
  assert.deepEqual([...first.read(0x27c0, 3)], [9, 8, 7]);
  assert.equal(registry('RegCloseKey', [key]), 0);
  const savedRegistry = first.takeRegistryChanges();
  assert.deepEqual(JSON.parse(savedRegistry)['hkey_current_user\\software\\simstest'].test, { t: 3, d: 'CQgH' });
  assert.equal(first.takeRegistryChanges(), undefined);
  first.importRegistry(savedRegistry);
  assert.equal(first.takeRegistryChanges(), undefined);
  assert.throws(() => first.importRegistry('{broken'), /snapshot/);
  assert.equal(first.restoreJournal(new Uint8Array()), 0);
  assert.equal(first.takeFileChanges().length, 0);
  const initializer = JSON.parse(first.resolve('msvcrt.dll', '_initterm'));
  assert.equal(initializer.callback, true);
  assert.equal(initializer.stackArgs, 0);
  const pointers = new DataView(guest.buffer);
  [0x401000, 0, 0x402000].forEach((pointer, index) => pointers.setUint32(0x2800 + index * 4, pointer, true));
  let continuation = JSON.parse(first.beginCall(initializer.id, Uint32Array.of(0x2800, 0x280c)));
  assert.equal(continuation.kind, 'callback');
  assert.equal(continuation.address, 0x401000);
  const consumed = continuation.token;
  continuation = JSON.parse(first.resumeCall(continuation.token, 0));
  assert.equal(continuation.address, 0x402000);
  assert.deepEqual(JSON.parse(first.resumeCall(continuation.token, 0)), { kind: 'return', value: 0 });
  assert.throws(() => first.resumeCall(consumed, 0));
  assert.equal(first.cancelCall(consumed), false);
  first.heapFree(replacement);
  assert.throws(() => first.heapFree(replacement), /live allocation/);
} finally {
  first.free();
}
const second = new Process(new WebAssembly.Memory({ initial: 1, maximum: 4 }), 0x10000, 0x40000);
try {
  const counter = JSON.parse(second.resolve('kernel32.dll', 'QueryPerformanceCounter'));
  assert.equal(counter.clock, true);
  const counterAddress = second.heapAlloc(8);
  assert.throws(() => second.call(counter.id, Uint32Array.of(counterAddress)), /sample|clock|time/i);
  assert.equal(second.call(counter.id, Uint32Array.of(counterAddress), 1234.5), 1);
  assert.equal(new DataView(second.read(counterAddress, 8).buffer).getBigUint64(0, true), 1234500n);
  const address = second.heapAlloc(4);
  assert.deepEqual([...second.read(address, 4)], [0, 0, 0, 0]);
  const alloc = JSON.parse(second.resolve('kernel32.dll', 'VirtualAlloc')).id;
  const reserved = second.call(alloc, Uint32Array.of(0, 8192, 0x2000, 4));
  assert.throws(() => second.read(reserved, 1), /not mapped/);
  assert.equal(second.call(alloc, Uint32Array.of(reserved, 4096, 0x1000, 4)), reserved);
  second.write(reserved, Uint8Array.of(9, 8, 7));
  second.call(alloc, Uint32Array.of(reserved, 8192, 0x1000, 4));
  assert.deepEqual([...second.read(reserved, 3)], [9, 8, 7]);
  assert.deepEqual([...second.read(reserved + 4096, 3)], [0, 0, 0]);
} finally { second.free(); }
// A failed memory.grow must not prevent relocation into already-backed free space.
const cappedMemory = new WebAssembly.Memory({ initial: 2, maximum: 2 });
const cappedProcess = new Process(cappedMemory, 0x10000, 0x40000);
try {
  const hole = cappedProcess.heapAlloc(0x8000);
  cappedProcess.write(hole, new Uint8Array(0x8000).fill(0xa5));
  cappedProcess.heapAlloc(0x7000);
  const frontier = cappedProcess.heapAlloc(0x1000);
  assert.equal(frontier, 0x1f000);
  cappedProcess.write(frontier, Uint8Array.of(4, 3, 2, 1));
  cappedProcess.heapFree(hole);
  cappedMemory.grow = () => { throw new Error('unrelated host growth failure'); };
  assert.throws(() => cappedProcess.heapRealloc(frontier, 0x2000), /unrelated host growth failure/);
  assert.equal(cappedProcess.heapSize(frontier), 0x1000);
  assert.deepEqual([...cappedProcess.read(frontier, 4)], [4, 3, 2, 1]);
  delete cappedMemory.grow;
  const relocated = cappedProcess.heapRealloc(frontier, 0x2000);
  assert.equal(relocated, hole, 'realloc reuses backed free space after in-place growth runs out of memory');
  assert.equal(cappedMemory.buffer.byteLength, 0x20000);
  assert.deepEqual([...cappedProcess.read(relocated, 4)], [4, 3, 2, 1]);
  assert.deepEqual([...cappedProcess.read(relocated + 0x1000, 0x1000)], new Array(0x1000).fill(0));
  assert.throws(() => cappedProcess.heapSize(frontier), /live allocation/);
} finally { cappedProcess.free(); }
const growingMemory = new WebAssembly.Memory({ initial: 1, maximum: 3 });
const growingProcess = new Process(growingMemory, 0x10000, 0x30000);
try {
  growingProcess.map(0x1000, Uint8Array.of(1, 2, 3, 4), true);
  const detached = growingMemory.buffer;
  growingMemory.grow(1);
  assert.equal(detached.byteLength, 0);
  assert.deepEqual([...growingProcess.read(0x1000, 4)], [1, 2, 3, 4]);
  growingProcess.write(0x1000, Uint8Array.of(4, 3, 2, 1));
  assert.deepEqual([...new Uint8Array(growingMemory.buffer, 0x1000, 4)], [4, 3, 2, 1]);
  growingProcess.adopt(0x10000, 4, true);
  growingProcess.write(0x10000, Uint8Array.of(9, 8, 7, 6));
  assert.deepEqual([...growingProcess.read(0x10000, 4)], [9, 8, 7, 6]);
} finally { growingProcess.free(); }
console.log('Runtime Wasm memory isolation, protection, allocation, OOM recovery, dispatch, and lifecycle checks passed');

// COM objects, guest Lock buffers and frame copies cross the actual Wasm boundary.
const displayMemory = new WebAssembly.Memory({initial: 4, maximum: 32});
const display = new Process(displayMemory, 0x10000, 0x200000);
try {
  const methods = JSON.parse(display.imports()).filter(api => api.dll === 'ddraw.dll');
  display.installModules(JSON.stringify({images:[{base:0x400000,sizeOfImage:4096,isDll:false}],nativeDlls:['ddraw.dll'],nativeBindings:methods.map(api=>({dll:api.dll,name:api.name,address:0x800000+api.id*4}))}));
  const invoke = (name,args) => display.call(methods.find(api=>api.name===name).id,Uint32Array.from(args));
  const scratch=display.heapAlloc(512);
  const word=(address,value)=>new DataView(displayMemory.buffer).setUint32(address,value,true);
  const read=address=>new DataView(displayMemory.buffer).getUint32(address,true);
  assert.equal(invoke('DirectDrawCreate',[0,scratch,0]),0);
  const device=read(scratch);
  for(const [offset,value] of [[0,108],[4,0x1007],[8,1],[12,2],[72,32],[76,0x40],[84,16],[104,0x200]]) word(scratch+offset,value);
  assert.equal(invoke('DDraw_CreateSurface',[device,scratch,scratch+128,0]),0);
  const surface=read(scratch+128);
  word(scratch+256,108);
  assert.equal(invoke('Surface_Lock',[surface,0,scratch+256,0,0]),0);
  display.write(read(scratch+292),Uint8Array.of(0,248,224,7));
  assert.equal(invoke('Surface_Unlock',[surface,0]),0);
  const frame=display.takeFrame();
  assert.equal(frame.width,2); assert.equal(frame.height,1);
  const rgba=frame.pixels(); frame.free();
  assert.deepEqual([...rgba],[255,0,0,255,0,255,0,255]);
  assert.equal(display.takeFrame(),undefined);
  assert.equal(invoke('Surface_Lock',[surface,0,scratch+256,0,0]),0);
  display.write(read(scratch+292),Uint8Array.of(31,0,255,255));
  assert.equal(invoke('Surface_Unlock',[surface,0]),0);
  const packed=display.takePackedFrame();
  assert.equal(packed.width,2); assert.equal(packed.height,1);
  assert.equal(packed.pitch,4); assert.equal(packed.format,16);
  assert.equal(display.takePackedFrame(),undefined);
  assert.equal(display.takeFrame(),undefined);
  assert.equal(invoke('Surface_Lock',[surface,0,scratch+256,0,0]),0);
  display.write(read(scratch+292),Uint8Array.of(0,0,0,0));
  assert.equal(invoke('Surface_Unlock',[surface,0]),0);
  const later=display.takeFrame();
  assert.deepEqual([...later.pixels()],[0,0,0,255,0,0,0,255]);
  later.free();
  assert.equal(invoke('Surface_Release',[surface]),0);
  const raw=packed.pixels();
  assert.deepEqual([...raw],[31,0,255,255]);
  raw.fill(0);
  assert.deepEqual([...packed.pixels()],[31,0,255,255]);
  packed.free();
  display.mouseInput(10,20,1); display.keyInput(65,true,false);
} finally { display.free(); }
console.log('DirectDraw guest locks and frame ownership: passed');

const { surfaceLockSmoke } = await import('./surface-lock.mjs');
surfaceLockSmoke(Process);

const { audioSmoke } = await import('./audio.mjs');
await audioSmoke(Process);

const { mediaSmoke } = await import('./media.mjs');
mediaSmoke(Process);
const { threadSmoke } = await import('./threads.mjs');
await threadSmoke(Process, moduleUrl);

const { commandLineSmoke } = await import('./command-line.mjs');
await commandLineSmoke(moduleUrl);

const { compilerMetadataSmoke } = await import('../../../compiler/tests/browser/metadata.mjs');
await compilerMetadataSmoke(moduleUrl);

const { frameOutputSmoke } = await import('./frame-output.mjs');
await frameOutputSmoke(moduleUrl);

const { keyboardSmoke } = await import('./keyboard.mjs');
await keyboardSmoke(moduleUrl);

const { storageSmoke, resetSmoke } = await import('./storage.mjs');
await storageSmoke(Process, moduleUrl);

await resetSmoke(moduleUrl);

const { assetCacheSmoke } = await import('./asset-cache.mjs');
await assetCacheSmoke(moduleUrl);
const { checksumSmoke } = await import('./checksum.mjs');
await checksumSmoke(moduleUrl);

const { optimizationSmoke } = await import('./optimization.mjs');
await optimizationSmoke(Process, moduleUrl);

const { bridgeSmoke } = await import('./bridge.mjs');
await bridgeSmoke(Process, moduleUrl);

const { threadBridgeSmoke } = await import('./thread-bridge.mjs');
threadBridgeSmoke(Process);

const { syncImportsSmoke } = await import('./sync-imports.mjs');
await syncImportsSmoke(moduleUrl);

const { nativeImportsSmoke } = await import('./native-imports.mjs');
await nativeImportsSmoke(Process, moduleUrl);

const { readyWaitsSmoke } = await import('./ready-waits.mjs');
readyWaitsSmoke(Process);

const { conditionalSuspensionSmoke } = await import('./conditional-suspension.mjs');
await conditionalSuspensionSmoke(moduleUrl);

await import('./guest-memory.mjs');

const { cleanupSmoke } = await import('./cleanup.mjs');
await cleanupSmoke(moduleUrl);

const { continuationCleanupSmoke } = await import('./continuation-cleanup.mjs');
await continuationCleanupSmoke(moduleUrl);

const { reentrySmoke } = await import('./reentry.mjs');
await reentrySmoke(Process, moduleUrl);

const { performanceSmoke } = await import('./performance.mjs');
await performanceSmoke(moduleUrl);

const { pwaSmoke } = await import('./pwa.mjs');
await pwaSmoke(moduleUrl);
