import assert from 'node:assert/strict';

export function readyWaitsSmoke(Process) {
  const owner = new Process(new WebAssembly.Memory({ initial: 4, maximum: 64 }), 0x10000, 0x400000);
  try {
    const api = name => JSON.parse(owner.resolve('kernel32.dll', name));
    const call = (name, args) => owner.call(api(name).id, Uint32Array.from(args));
    const schedule = (name, args, now) => JSON.parse(owner.scheduleCall(api(name).id, Uint32Array.from(args), now));
    assert.equal(owner.hasReadyWaits(), false);
    const event = call('CreateEventA', [0, 0, 0, 0]);
    const waiting = schedule('WaitForSingleObject', [event, 50], 100);
    assert.equal(waiting.kind, 'wait');
    assert.equal(owner.hasReadyWaits(), false);
    call('SetEvent', [event]);
    assert.equal(owner.hasReadyWaits(), true, 'wake APIs expose ready results without another clock update');
    assert.equal(owner.hasReadyWaits(), true, 'query does not drain');
    const ready = JSON.parse(owner.takeReadyWaits());
    assert.equal(ready.length, 1); assert.equal(ready[0].token, waiting.token); assert.equal(ready[0].value, 0);
    assert.equal(owner.hasReadyWaits(), false);
    const sleeping = schedule('Sleep', [10], 200);
    assert.equal(owner.hasReadyWaits(), false);
    owner.advanceTime(209); assert.equal(owner.hasReadyWaits(), false);
    owner.advanceTime(210); assert.equal(owner.hasReadyWaits(), true);
    assert.equal(owner.cancelWait(sleeping.token), true);
    assert.equal(owner.hasReadyWaits(), false);
    assert.deepEqual(JSON.parse(owner.takeReadyWaits()), []);
  } finally { owner.free(); }
  console.log('Ready-wait query: immediate wake visibility, non-consuming reads, timer advancement and cancellation passed');
}
