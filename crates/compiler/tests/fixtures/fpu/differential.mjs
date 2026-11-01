import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
const module = new WebAssembly.Module(readFileSync(0));
const names = ['eax','ecx','edx','ebx','esp','ebp','esi','edi','flags','fp_top','fp_status','trap_va','call_target','resume_next','actual_return','teb_base'];
const create = () => {
  const env = {memory:new WebAssembly.Memory({initial:1}), observe(){}};
  for(const name of names) env[name] = new WebAssembly.Global({value:'i32',mutable:true},0);
  return {env, guest:new WebAssembly.Instance(module,{env}).exports, bytes:new Uint8Array(env.memory.buffer)};
};
const plain=create(), fast=create();
const patterns=[0n,0x8000000000000000n,1n,0x8000000000000001n,0x3ff0000000000000n,0xc004000000000000n,0x7ff0000000000000n,0xfff0000000000000n,0x7ff8000000001234n,0xfff0000000004321n,0x7fefffffffffffffn];
const initial=new Uint8Array(65536), view=new DataView(initial.buffer);
let seed=0x12345678;
for(let at=0;at<initial.length;at+=4){seed=(Math.imul(seed,1664525)+1013904223)|0;view.setInt32(at,seed,true);}
const routines=Object.keys(plain.guest).filter(name=>name.endsWith('_plain')).map(name=>name.slice(0,-6));
let checks=0;
for(const routine of routines) for(const top of [0,1,2,3,4,5,6,7,8,-1,0x7fffffff,0x80000000]) {
  const rotation=top&7;
  const cases=[
    [0x2001,0x3003,0x4005,0x1003], [0x2000,0x2000,0x2000,0x1000],
    [0x2000,0x2001,0x2004,0x1000], [0x2000,0x2004,0x2007,0x1000],
    [0x2000,0xffff,0x4000,0x1000], [0xfffc,0x3000,0x4000,0x1000],
    [0xfffffffcn,0x3000,0x4000,0x1000].map(Number),
    [0x2000,0x3000,0x4000,0xfbC0], [0x2000,0x3000,0x4000,0xfbC1],
    [0x2000,0x3000,0x4000,0xfffffc00],
  ];
  for(const offset of [-8,-7,-4,-1,0,1,7,8,55,63,64]) {
    cases.push([0x1400+offset,0x3000,0x4000,0x1000]);
    cases.push([0x2000,0x1400+offset,0x4000,0x1000]);
  }
  for(const [eax,ecx,edx,teb] of cases) {
    const bank=(teb+0x400)>>>0;
    const bytes=initial.slice(), data=new DataView(bytes.buffer);
    for(let i=0;i<8;i++) if(bank+i*8+8<=bytes.length) data.setBigUint64(bank+i*8,patterns[(rotation+i)%patterns.length],true);
    // Include exact quiet/signaling NaN and signed-zero external operands.
    if(eax+8<=bytes.length) data.setBigUint64(eax,patterns[(rotation+3)%patterns.length],true);
    const outcomes=[];
    for(const [host,suffix] of [[plain,'plain'],[fast,'fast']]) {
      host.bytes.set(bytes);
      for(const [i,name] of names.entries())host.env[name].value=(i*0x13579+0x1234)|0;
      for(const [name,value] of Object.entries({eax,ecx,edx,teb_base:teb,fp_top:top}))host.env[name].value=value;
      let trap=null;
      try{host.guest[`${routine}_${suffix}`]();}catch(error){assert.ok(error instanceof WebAssembly.RuntimeError);trap=error.message;}
      outcomes.push({trap,registers:names.map(name=>host.env[name].value),bytes:host.bytes.slice()});
    }
    assert.deepEqual(outcomes[1],outcomes[0],`${routine} top=${top} eax=${eax.toString(16)} ecx=${ecx.toString(16)} teb=${teb.toString(16)}`);
    checks++;
  }
}
console.log(`FPU forwarding: ${checks} full-memory/register/fault comparisons passed`);
