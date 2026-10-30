import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
const names=['eax','ecx','edx','ebx','esp','ebp','esi','edi','flags','fp_top','fp_status','trap_va','call_target','resume_next','actual_return','teb_base'];
const env={memory:new WebAssembly.Memory({initial:1}),observe(){}};
for(const name of names)env[name]=new WebAssembly.Global({value:'i32',mutable:true},0);
const {exports}=new WebAssembly.Instance(new WebAssembly.Module(readFileSync(0)),{env});
let count=0;
for(const {name,cases} of fixture.routines) {
 for(const [a,b,initial,expected,flags] of cases) {
  env.eax.value=a;env.ecx.value=b;env.flags.value=initial&0x8d5;
  exports[name]();
  // Auxiliary carry is outside the full-width implementation's contract.
  const eaxMask=name==='compare_lahf'?~0x1000:0xffffffff;
  assert.equal(env.eax.value&eaxMask,expected&eaxMask,name+' result');
  assert.equal(env.flags.value&fixture.provenance.flagMask,flags&fixture.provenance.flagMask,name+' flags');
  count++;
 }
}
assert.equal(count,672);
