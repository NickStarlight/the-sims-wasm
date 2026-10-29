// This test host is deliberately independent of compatibility API dispatch.
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
const input = JSON.parse(readFileSync(0, 'utf8'));
assert.equal(typeof WebAssembly.Suspending, 'function', 'test requires JSPI-capable V8');
const guest = new WebAssembly.Module(Uint8Array.from(input.guest));
const native = new WebAssembly.Module(Uint8Array.from(input.native));

async function execute(suspend) {
  const env = {
    memory: new WebAssembly.Memory({ initial: 256, maximum: 65536 }),
    table: new WebAssembly.Table({ element: 'anyfunc', initial: 300000 }),
    dispatch() { assert.fail('direct native API test must never dispatch through JS'); },
  };
  for (const name of input.registers) env[name] = new WebAssembly.Global({ value: 'i32', mutable: true }, 0);
  const words = new Uint32Array(env.memory.buffer);
  new Uint8Array(env.memory.buffer).fill(0xcc, 0x20000, 0x21000);
  const stack = 0x18000;
  env.esp.value = stack;
  env.esi.value = 0x10000;
  words[stack / 4] = 0;
  let checkpoints = 0;
  let eventLoopTurns = 0;
  const visits = new Map();
  const repeatCounts = [];
  const snapshot = () => input.registers.map(name => env[name].value);
  function checkpoint(va) {
    assert.equal(env.trap_va.value, va);
    checkpoints++;
    visits.set(va, (visits.get(va) ?? 0) + 1);
    if (va === 0x40130a) repeatCounts.push(env.ecx.value);
    if (!suspend || checkpoints % 7 !== 0) return;
    const savedRegisters = snapshot();
    const savedStack = words.slice(stack / 4 - 16, stack / 4 + 4);
    return new Promise(resolve => setImmediate(() => {
      eventLoopTurns++;
      assert.deepEqual(snapshot(), savedRegisters, 'registers changed during suspension');
      assert.deepEqual(words.slice(stack / 4 - 16, stack / 4 + 4), savedStack, 'guest stack changed during suspension');
      resolve();
    }));
  }
  env.blockTrace = suspend ? new WebAssembly.Suspending(checkpoint) : checkpoint;
  const instance = new WebAssembly.Instance(guest, { env });
  const api = new WebAssembly.Instance(native, { env });
  env.table.set(input.apiSlot, api.exports.increment);
  // idxOf's sorted guest address table belongs to the execution host.
  words.set([0x401000, 0x401100, 0x401200, 0x401300, 0x401400], 0xa00000 / 4);
  if (suspend) await WebAssembly.promising(instance.exports.runAt)(0x401000);
  else instance.exports.runAt(0x401000);
  assert.equal(env.eax.value, 1024 + 1024 + 700 + 7);
  assert.equal(env.ebx.value, 0x12345678);
  assert.equal(env.ecx.value, 0);
  assert.equal(env.esp.value, stack + 4, 'all nested frames and the top-level return were popped');
  assert.equal(env.esi.value, 0x10000);
  assert.equal(env.edi.value, 0x21000);
  assert.ok(visits.get(0x401108) > 0 && visits.get(0x401108) < 1023, 'tight loop must poll without crossing into JS every backedge');
  assert.ok(visits.get(0x401206) > 0 && visits.get(0x401206) < 1023, 'direct API loop must poll independently of dispatch');
  assert.deepEqual(repeatCounts, [4096, 3072, 2048, 1024]);
  for (const va of [0x401500, 0x401600]) assert.ok(visits.has(va), 'long straight-line code needs checkpoints');
  const bytes = new Uint8Array(env.memory.buffer);
  assert.ok(bytes.slice(0x20000, 0x21000).every(byte => byte === 0));
  if (suspend) assert.ok(eventLoopTurns > 0, 'guest must actually release the host event loop');
  return { registers: snapshot(), stack: words.slice(stack / 4 - 16, stack / 4 + 4), checkpoints };
}
assert.deepEqual(await execute(true), await execute(false), 'suspension changed guest behavior');
