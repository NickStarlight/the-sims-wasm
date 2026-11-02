import assert from 'node:assert/strict';

export async function mediaSmoke(Process, directory) {
  const owner=new Process(new WebAssembly.Memory({initial:4,maximum:64}),0x10000,0x400000);
  try {
    const api=name=>JSON.parse(owner.resolve('winmm',name));
    const call=(name,args,time)=>{const entry=api(name);return owner.call(entry.id,Uint32Array.from(args),time);};
    assert.equal(api('timeSetEvent').clock,true);
    const timer=call('timeSetEvent',[10,1,0x401000,123,1],0);
    assert.equal(owner.nextMediaTimer(9),undefined);
    assert.deepEqual(JSON.parse(owner.nextMediaTimer(10)),{address:0x401000,args:[timer,0,123,0,0]});
    assert.equal(owner.nextMediaTimer(10),undefined);
    assert.equal(call('timeKillEvent',[timer]),0);
    assert.equal(owner.nextMediaTimer(20),undefined);
    // MP3 streaming reads ordinary files, so message polling must service its
    // WinMM refill timer even when no MMIO operation occurs.
    const { ApiDispatcher } = await import(new URL('./runtime/dispatch.js', directory));
    const memory = new WebAssembly.Memory({initial:1});
    const registers = Object.fromEntries(['esp','eax','actual_return'].map(name =>
      [name, new WebAssembly.Global({value:'i32',mutable:true},0)]));
    const host = new ApiDispatcher({process:owner,memory,registers,threadId:1}, {}, () => {}, () => {});
    const timerApi = api('timeSetEvent');
    const periodic = owner.call(timerApi.id, Uint32Array.from([10,1,0x401000,456,1]), performance.now()-20);
    let fired = 0;
    host.callGuest = async (address,args) => {
      assert.equal(address,0x401000);
      assert.deepEqual(args,[periodic,0,456,0,0]);
      fired++;
      call('timeKillEvent',[periodic]); // Cancellation prevents catch-up/reentry.
    };
    for (const name of ['PeekMessageA','GetMessageA']) {
      const entry = JSON.parse(owner.resolve('user32',name));
      assert.equal(entry.media,true, `${name} must use suspending timer dispatch`);
    }
    const entry = JSON.parse(owner.resolve('user32','PeekMessageA'));
    const message = owner.heapAlloc(28);
    registers.esp.value = 0x1000;
    [0x12345678,message,0,0,0,0].forEach((v,i)=>new DataView(memory.buffer).setUint32(0x1000+4*i,v,true));
    await host.callImport({label:'user32.dll!PeekMessageA',api:entry});
    assert.equal(fired,1);
    assert.equal(registers.esp.value,0x1018);
    assert.equal(registers.actual_return.value,0x12345678);
    owner.addFile('audio/test.wav',Uint8Array.of(82,73,70,70,1,2,3));
    const path=owner.heapAlloc(32),out=owner.heapAlloc(16);
    owner.write(path,new TextEncoder().encode('audio/test.wav\0'));
    assert.equal(api('mmioOpenA').media,true);
    const file=call('mmioOpenA',[path,0,0]);assert.notEqual(file,0);
    assert.equal(call('mmioRead',[file,out,4]),4);assert.deepEqual([...owner.read(out,4)],[82,73,70,70]);
    assert.equal(call('mmioSeek',[file,-1,2]),6);
    assert.equal(call('mmioRead',[file,out,8]),1);assert.equal(owner.read(out,1)[0],3);
    assert.equal(call('mmioClose',[file,0]),0);
  } finally { owner.free(); }
  console.log('WinMM timer callback transport and MMIO Wasm cursor parity: passed');
}
