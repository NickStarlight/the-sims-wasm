// Execute two independently instantiated, Rust-compiled x86 threads through the production host.
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';

export async function threadSmoke(Process, directory) {
  const { default: init, Compiler } = await import(new URL('./compiler.js', directory));
  await init({ module_or_path: readFileSync(new URL('./compiler_bg.wasm', directory)) });
  const { ThreadHost } = await import(new URL('./threads.js', directory));
  const work = { clock: 0, ready: 0, runnable: 0 };
  let ready = [];
  const idle = new ThreadHost({
    scheduleCall: () => '{"kind":"return","value":0}',
    advanceTime() { work.clock++; },
    takeReadyWaits() { work.ready++; return JSON.stringify(ready.splice(0)); },
    takeRunnableThreads() { work.runnable++; return '[]'; },
  });
  idle.notify('LeaveCriticalSection');
  assert.equal(idle.schedule(1, []), 0);
  await Promise.resolve();
  assert.deepEqual(work, { clock: 0, ready: 0, runnable: 0 }, 'uncontended calls do not scan scheduler queues');
  let woke = false;
  idle.waits.set(1, { resolve(value) { woke = value === 7; }, deadline: null });
  ready.push({ token: 1, value: 7 });
  idle.notify('SetEvent');
  await Promise.resolve();
  assert.ok(woke, 'wake notifications still complete pending waits');
  assert.equal(work.runnable, 0, 'ordinary wake does not scan thread launch queue');
  idle.notify('CreateThread');
  await Promise.resolve();
  idle.notify('ResumeThread');
  await Promise.resolve();
  assert.equal(work.runnable, 2, 'thread creation/resumption must launch even without waits');
  await idle.close();
  const cancelled = new ThreadHost({});
  cancelled.checkAbort();
  const reason = new Error('external abort');
  cancelled.cancellation.abort(reason);
  assert.throws(() => cancelled.checkAbort(), error => error === reason, 'cached abort state updates synchronously and retains the exact reason');
  await cancelled.close();
  const quiet = new ThreadHost({
    hasReadyWaits: () => false,
    advanceTime() { assert.fail('unrelated wakes must not poll empty kernel queues'); },
    cancelWait() {},
  });
  quiet.waits.set(1, { deadline: null, reject() {} });
  quiet.notify('LeaveCriticalSection'); quiet.notify('SetEvent');
  await quiet.close();
  const originalTimeout = globalThis.setTimeout, originalClear = globalThis.clearTimeout;
  let armed = 0, cleared = 0;
  const timed = new ThreadHost({ advanceTime() {}, takeReadyWaits: () => '[]', takeRunnableThreads: () => '[]' });
  try {
    globalThis.setTimeout = () => ++armed;
    globalThis.clearTimeout = id => { if (id !== null) cleared++; };
    timed.waits.set(1, { deadline: performance.now() + 10000 });
    for (let i = 0; i < 500; i++) timed.notify('LeaveCriticalSection');
    assert.equal(armed, 1, 'repeated unrelated wakes must retain the same deadline timer');
    assert.equal(cleared, 0);
    timed.waits.set(2, { deadline: performance.now() + 1000 });
    timed.notify('SetEvent');
    assert.equal(armed, 2, 'an earlier deadline must rearm the timer');
    timed.waits.clear(); timed.drain(false);
    assert.equal(timed.timer, null);
  } finally {
    await timed.close();
    globalThis.setTimeout = originalTimeout; globalThis.clearTimeout = originalClear;
  }
  const { prepareImports } = await import(new URL('./linker.js', directory));
  const memory = new WebAssembly.Memory({ initial: 16, maximum: 1024 });
  const owner = new Process(memory, 0x1000000, 0x4000000);
  let host;
  try {
    owner.adopt(0x1000, 4096, true);
    const manifest = JSON.parse(owner.imports());
    const api = name => manifest.find(api => api.dll === 'kernel32.dll' && api.name === name);
    const call = (name, args) => owner.call(api(name).id, Uint32Array.from(args));
    const address = name => 0x800000 + manifest.indexOf(api(name)) * 4;
    const code = [0x53, 0x8b, 0x5c, 0x24, 8]; // preserve EBX; EBX = thread parameter
    const invoke = name => code.push(0xb8, ...u32(address(name)), 0xff, 0xd0);
    invoke('GetCurrentThreadId');
    code.push(0x89, 0x43, 4, 0x50, 0xff, 0x73, 8); // save ID, push value/slot
    invoke('TlsSetValue');
    code.push(0x6a, 0xff, 0xff, 0x73, 20); // wait on shared mutex, INFINITE
    invoke('WaitForSingleObject');
    code.push(0x89, 0x43, 24, 0x8b, 0x43, 4, 0xa3, ...u32(0x1180)); // record wait; shared owner = ID
    code.push(0xff, 0x73, 12);
    invoke('Sleep');
    code.push(0xa1, ...u32(0x1180), 0x89, 0x43, 28); // owner must survive the other thread's attempted wait
    code.push(0xff, 0x73, 20);
    invoke('ReleaseMutex');
    code.push(0xff, 0x73, 8);
    invoke('TlsGetValue');
    code.push(0x89, 0x43, 16, 0xff, 0x33);
    invoke('SetEvent');
    code.push(0x8b, 0x43, 4, 0x5b, 0xc2, 4, 0); // return thread ID, stdcall
    const compiler = new Compiler(executable(code));
    let artifact;
    let bytes, metadata, plan, image;
    try {
      artifact = compiler.compile(new Uint32Array());
      bytes = artifact.wasm(); metadata = JSON.parse(artifact.metadataJson());
      plan = JSON.parse(compiler.loadPlanJson()); image = compiler.loadBytes();
      plan.images[0].path = 'C:\\Sims\\Sims.exe';
    } finally { artifact?.free(); compiler.free(); }
    owner.adopt(metadata.tableAddr, (metadata.funcCount + 4) * 4, true);
    for (const item of plan.images) owner.adopt(item.base, item.sizeOfImage, true);
    for (const span of plan.spans) owner.write(span.address, image.subarray(span.offset, span.offset + span.length));
    const put = (p, value) => owner.write(p, Uint8Array.from(u32(value)));
    const get = p => new DataView(owner.read(p, 4).buffer).getUint32(0, true);
    metadata.funcAddrs.forEach((value, i) => put(metadata.tableAddr + i * 4, value));
    const imports = prepareImports(owner, plan, metadata, memory);
    const module = await WebAssembly.compile(bytes);
    host = new ThreadHost(owner, memory, module, metadata, imports, { flush() {} }, () => {}, () => {}, () => {});
    owner.setThread(1);
    const slot = call('TlsAlloc', []);
    call('TlsSetValue', [slot, 99]);
    const mutex = call('CreateMutexA', [0, 0, 0]);
    const handles = [];
    for (let i = 0; i < 2; i++) {
      const p = 0x1000 + i * 32;
      put(p, call('CreateEventA', [0, 1, 0, 0])); put(p + 8, slot); put(p + 12, 5 + i * 5);
      put(p + 20, mutex);
      handles.push(call('CreateThread', [0, 4096, metadata.entry, p, 4, 0]));
    }
    assert.deepEqual(JSON.parse(owner.takeRunnableThreads()), []);
    for (const handle of handles) assert.equal(call('ResumeThread', [handle]), 1);
    host.drain();
    const joins = handles.map(handle => { owner.setThread(1); return host.schedule(api('WaitForSingleObject').id, Uint32Array.of(handle, 1000)); });
    assert.deepEqual(await Promise.all(joins), [0, 0]);
    assert.equal(host.signal.aborted, false, host.signal.reason?.message);
    for (let i = 0; i < 2; i++) {
      const p = 0x1000 + i * 32;
      assert.equal(get(p + 4), i + 2, 'each instance reads its own TEB');
      assert.equal(get(p + 16), i + 2, 'TLS survives another thread running during Sleep');
      assert.equal(get(p + 24), 0, 'mutex acquisition returns WAIT_OBJECT_0');
      assert.equal(get(p + 28), i + 2, 'mutex preserves exclusive ownership across JSPI suspension');
      assert.equal(call('GetExitCodeThread', [handles[i], 0x1100]), 1);
      assert.equal(get(0x1100), i + 2);
    }
    owner.setThread(1);
    assert.equal(call('TlsGetValue', [slot]), 99, 'main thread TLS is isolated');
    assert.equal(call('CloseHandle', [mutex]), 1);
  } finally { await host?.close(); owner.free(); }
  console.log('Cooperative x86 threads: independent instances/registers, TLS, mutex wakeups, sleep and joins passed');
}
const u32 = value => [...new Uint8Array(Uint32Array.of(value).buffer)];
function executable(code) {
  const bytes = new Uint8Array(0x400), view = new DataView(bytes.buffer);
  const word = (at, value) => view.setUint16(at, value, true);
  const dword = (at, value) => view.setUint32(at, value, true);
  word(0, 0x5a4d); dword(0x3c, 0x80); dword(0x80, 0x4550);
  word(0x84, 0x14c); word(0x86, 1); word(0x94, 0xe0); word(0x98, 0x10b);
  dword(0xa8, 0x1000); dword(0xb4, 0x400000); dword(0xd0, 0x2000); dword(0xd4, 0x200);
  bytes.set(new TextEncoder().encode('.text'), 0x178);
  for (const [at, value] of [[0x180, 0x200], [0x184, 0x1000], [0x188, 0x200], [0x18c, 0x200], [0x19c, 0x60000020]]) dword(at, value);
  bytes.set(code, 0x200);
  return bytes;
}
