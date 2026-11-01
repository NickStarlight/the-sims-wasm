import assert from 'node:assert/strict';

export function mediaSmoke(Process) {
  const owner=new Process(new WebAssembly.Memory({initial:4,maximum:64}),0x10000,0x400000);
  try {
    const api=name=>JSON.parse(owner.resolve('winmm',name));
    const call=(name,args,time)=>{const entry=api(name);return owner.call(entry.id,Uint32Array.from(args),time);};
    assert.equal(api('timeSetEvent').clock,true);
    const timer=call('timeSetEvent',[10,1,0x401000,123,1],1000);
    assert.equal(owner.nextMediaTimer(1009),undefined);
    assert.deepEqual(JSON.parse(owner.nextMediaTimer(1010)),{address:0x401000,args:[timer,0,123,0,0]});
    assert.equal(owner.nextMediaTimer(1010),undefined);
    assert.equal(call('timeKillEvent',[timer]),0);
    assert.equal(owner.nextMediaTimer(2000),undefined);
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
