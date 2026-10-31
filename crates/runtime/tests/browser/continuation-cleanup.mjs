import assert from 'node:assert/strict';

// Exercise the production continuation hosts with a real Wasm trap as the
// primary error and a simulated poisoned binding during cleanup.
export async function continuationCleanupSmoke(directory) {
  const { completeCall } = await import(new URL('./callbacks.js', directory));
  const { dispatchException } = await import(new URL('./exceptions.js', directory));
  const { registerNames } = await import(new URL('./machine.js', directory));
  const trap = new WebAssembly.Instance(new WebAssembly.Module(Uint8Array.from([
    0, 97, 115, 109, 1, 0, 0, 0, 1, 4, 1, 96, 0, 0, 3, 2, 1, 0,
    7, 7, 1, 3, 114, 117, 110, 0, 0, 10, 5, 1, 3, 0, 0, 11,
  ]))).exports.run;
  const poison = () => new Error('recursive use of an object detected which would lead to unsafe aliasing in rust');
  const registers = () => Object.fromEntries(registerNames.map((name, i) => [
    name, new WebAssembly.Global({value: 'i32', mutable: true}, i * 17),
  ]));
  const action = values => ({kind: 'invoke', token: 71, address: 0x401000, registers: values});
  let cases = 0;

  for (const failureAt of ['invoke', 'resume']) {
    for (const cleanupAt of ['select', 'cancel', 'none']) {
      for (const host of ['callback', 'exception']) {
        let primary;
        const events = [];
        const fail = () => {
          try { trap(); } catch (error) { primary = error; throw error; }
        };
        const select = () => {
          events.push('select');
          if (primary && cleanupAt === 'select') throw poison();
        };
        const cancel = token => {
          events.push(['cancel', token]);
          if (cleanupAt === 'cancel') throw poison();
        };
        const invoke = () => {
          events.push('invoke');
          if (failureAt === 'invoke') fail();
          return 19;
        };
        const resume = (token, value) => {
          events.push(['resume', token]);
          assert.equal(token, 71);
          if (failureAt === 'resume') fail();
          return JSON.stringify({kind: 'return', value});
        };
        const request = {kind: 'callback', token: 71, address: 0x401000, args: [23]};
        const running = host === 'callback'
          ? completeCall({resumeCall: resume, cancelCall: cancel}, JSON.stringify(request), invoke, select)
          : dispatchException({
              process: {
                beginException() { return JSON.stringify(action(registerNames.map((_, i) => i * 31))); },
                resumeException: resume,
                cancelException: cancel,
              },
              registers: registers(),
              runRaw: invoke,
              selectThread: select,
            }, 1, Uint32Array.of(9));
        await assert.rejects(running, error => error === primary && error instanceof WebAssembly.RuntimeError,
          `${host}: ${failureAt} failure must survive ${cleanupAt} cleanup failure`);
        assert.equal(events.filter(event => event === 'select').length, failureAt === 'resume' ? 2 : 1);
        assert.equal(events.filter(event => Array.isArray(event) && event[0] === 'cancel').length,
          cleanupAt === 'select' ? 0 : 1, 'cancel only follows successful thread selection');
        cases++;
      }
    }
  }

  const selected = [];
  const pending = JSON.stringify({kind: 'callback', token: 73, address: 0x402000, args: [5, 7]});
  const result = await completeCall({
    resumeCall(token, value) {
      assert.equal(token, 73);
      assert.equal(value, 12);
      return JSON.stringify({kind: 'return', value: 99});
    },
    cancelCall() { assert.fail('successful callback must not be cancelled'); },
  }, pending, async (address, args) => {
    assert.equal(address, 0x402000);
    assert.deepEqual(args, [5, 7]);
    return args[0] + args[1];
  }, () => selected.push('callback'));
  assert.equal(result, 99);
  assert.deepEqual(selected, ['callback']);

  const state = registers();
  const invoked = registerNames.map((_, i) => i * 43);
  const completed = registerNames.map((_, i) => i * 59);
  await dispatchException({
    process: {
      beginException() { return JSON.stringify(action(invoked)); },
      resumeException(token, snapshot) {
        assert.equal(token, 71);
        assert.deepEqual([...snapshot], invoked);
        return JSON.stringify({kind: 'complete', registers: completed});
      },
      cancelException() { assert.fail('successful exception handler must not be cancelled'); },
    },
    registers: state,
    async runRaw(address) {
      assert.equal(address, 0x401000);
      assert.deepEqual(registerNames.map(name => state[name].value), invoked);
    },
    selectThread() { selected.push('exception'); },
  }, 1, Uint32Array.of(9));
  assert.deepEqual(registerNames.map(name => state[name].value), completed);
  assert.deepEqual(selected, ['callback', 'exception']);
  assert.equal(cases, 12);
  console.log('Continuation cleanup: original Wasm traps survive thread-selection/cancel failures; successful callbacks and SEH resume unchanged');
}
