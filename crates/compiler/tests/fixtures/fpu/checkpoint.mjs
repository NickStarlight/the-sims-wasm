import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
const input=JSON.parse(readFileSync(0,'utf8'));
const module=new WebAssembly.Module(Uint8Array.from(input.wasm));
for(const suspend of [false,true]){
  const env={memory:new WebAssembly.Memory({initial:256,maximum:65536}),table:new WebAssembly.Table({element:'anyfunc',initial:300000}),dispatch(){assert.fail('unexpected import')}};
  for(const name of input.names)env[name]=new WebAssembly.Global({value:'i32',mutable:true},0);
  const view=new DataView(env.memory.buffer), bank=0x1400;
  env.esp.value=0x18000;env.teb_base.value=0x1000;
  input.addresses.forEach((address,i)=>view.setUint32(input.table+i*4,address,true));
  let checkpoints=0;
  const observe=va=>{
    checkpoints++;
    assert.equal(va,0x401200);
    assert.equal(env.trap_va.value,va);
    assert.equal(env.fp_top.value,4);
    const expected=[1,1,1,2,3,1,1,1];
    assert.deepEqual(Array.from({length:8},(_,i)=>view.getFloat64(bank+i*8,true)),expected);
    // A host turn can change guest memory. The next run must read it again.
    const change=()=>view.setFloat64(bank+4*8,20,true);
    if(suspend)return new Promise(resolve=>setImmediate(()=>{change();resolve()}));
    change();
  };
  env.blockTrace=suspend?new WebAssembly.Suspending(observe):observe;
  const guest=new WebAssembly.Instance(module,{env}).exports;
  if(suspend)await WebAssembly.promising(guest.runAt)(0x401000);
  else guest.runAt(0x401000);
  assert.equal(checkpoints,1);
  assert.equal(env.fp_top.value,4);
  assert.equal(view.getFloat64(bank+4*8,true),21);
  assert.equal(env.esp.value,0x18004);
}
