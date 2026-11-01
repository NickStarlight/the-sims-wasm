#[test]
fn masked_integer_conversions_store_indefinite_and_preserve_stack_and_status() {
    crate::translate::test_support::execute_v8(
        &[
            ("word", vec![0xdf, 0x11]),      // FIST m16 [ECX]
            ("word_pop", vec![0xdf, 0x19]),  // FISTP m16 [ECX]
            ("dword", vec![0xdb, 0x11]),     // FIST m32 [ECX]
            ("dword_pop", vec![0xdb, 0x19]), // FISTP m32 [ECX]
            ("qword_pop", vec![0xdf, 0x39]), // FISTP m64 [ECX]
            ("compare", vec![0xd8, 0xd0]),   // FCOM ST0
            ("clear", vec![0xdb, 0xe2]),     // FNCLEX
            ("status", vec![0xdf, 0xe0]),    // FNSTSW AX
        ],
        r#"
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
const env={memory:new WebAssembly.Memory({initial:1})};
for(const name of ['eax','ecx','edx','ebx','esp','ebp','esi','edi','flags','fp_top','fp_status','trap_va','call_target','resume_next','actual_return','teb_base'])
  env[name]=new WebAssembly.Global({value:'i32',mutable:true},0);
const {exports}=new WebAssembly.Instance(new WebAssembly.Module(readFileSync(0)),{env});
const bytes=new Uint8Array(env.memory.buffer),view=new DataView(env.memory.buffer);
env.ecx.value=0x1000;env.teb_base.value=0x2000;
for(const [name,width,bound,indefinite] of [
 ['word',2,2**15,-(2**15)],['word_pop',2,2**15,-(2**15)],
 ['dword',4,2**31,-(2**31)],['dword_pop',4,2**31,-(2**31)],
 ['qword_pop',8,2**63,-(2n**63n)],
]) {
 const pop=name.endsWith('_pop');
 for(const value of [NaN,Infinity,-Infinity,bound,-bound*2,0,1,-1,-bound,bound/2]) {
  bytes.fill(0xa5,0xfff,0x100a);
  env.fp_top.value=7;env.fp_status.value=0x400;
  view.setFloat64(0x2438,value,true);
  exports[name]();
  const invalid=!Number.isFinite(value)||value < -bound||value >= bound;
  const expected=invalid?indefinite:width===8?BigInt(value):value;
  const stored=width===2?view.getInt16(0x1000,true):width===4?view.getInt32(0x1000,true):view.getBigInt64(0x1000,true);
  assert.equal(stored,expected,`${name}(${value})`);
  assert.equal(env.fp_top.value,pop?0:7,`${name} pop`);
  assert.equal(env.fp_status.value,0x400|(invalid?1:0));
  assert.equal(bytes[0xfff],0xa5);assert.equal(bytes[0x1000+width],0xa5);
 }
}
env.fp_top.value=0;env.fp_status.value=0x25;view.setFloat64(0x2400,1,true);
exports.compare();assert.equal(env.fp_status.value,0x4025,'comparison preserves sticky exceptions');
env.eax.value=0x12340000;exports.status();assert.equal(env.eax.value,0x12344025);
env.fp_status.value=0xc7ff;env.fp_top.value=5;exports.clear();
assert.equal(env.fp_status.value,0x4700);assert.equal(env.fp_top.value,5);
"#,
    );
}

#[test]
fn control_word_rounds_integer_results_and_survives_environment_save_restore() {
    crate::translate::test_support::execute_v8(
        &[
            ("load_control", vec![0xd9, 0x2a]),  // FLDCW [EDX]
            ("store_control", vec![0xd9, 0x3a]), // FNSTCW [EDX]
            ("round", vec![0xd9, 0xfc]),
            ("word", vec![0xdf, 0x11]),
            ("dword", vec![0xdb, 0x11]),
            ("qword", vec![0xdf, 0x39]),
            ("save", vec![0xdd, 0x32]),     // FNSAVE [EDX]
            ("restore", vec![0xdd, 0x22]),  // FRSTOR [EDX]
            ("save_env", vec![0xd9, 0x32]), // FNSTENV [EDX]
            ("load_env", vec![0xd9, 0x22]), // FLDENV [EDX]
            ("init", vec![0xdb, 0xe3]),
        ],
        r#"
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
const module=new WebAssembly.Module(readFileSync(0));
const env={memory:new WebAssembly.Memory({initial:1})};
for(const name of ['eax','ecx','edx','ebx','esp','ebp','esi','edi','flags','fp_top','fp_status','trap_va','call_target','resume_next','actual_return','teb_base'])
  env[name]=new WebAssembly.Global({value:'i32',mutable:true},0);
const {exports}=new WebAssembly.Instance(module,{env});
const view=new DataView(env.memory.buffer);
env.ecx.value=0x1000;env.edx.value=0x1100;env.teb_base.value=0x2000;
const mode=word=>{view.setUint16(0x1100,word,true);exports.load_control();};
const set=value=>{env.fp_top.value=0;env.fp_status.value=0;view.setFloat64(0x2400,value,true);};
const nearest=value=>{
 const low=Math.floor(value),fraction=value-low;
 const result=fraction<.5?low:fraction>.5?low+1:low%2===0?low:low+1;
 return result===0&&value<0?-0:result;
};
for(const [bits,round] of [[0,nearest],[0x400,Math.floor],[0x800,Math.ceil],[0xc00,Math.trunc]]) {
 mode(0x127f|bits);exports.store_control();assert.equal(view.getUint16(0x1100,true),0x127f|bits);
 for(const value of [3.5,-3.5,2.5,-2.5,.5,-.5,0,-0,Infinity,-Infinity,NaN]) {
  set(value);exports.round();const expected=Number.isFinite(value)?round(value):value;
  assert.equal(view.getFloat64(0x2400,true),expected,`FRNDINT ${bits} ${value}`);
  if(!Number.isFinite(value))continue;
  for(const [name,width] of [['word',2],['dword',4],['qword',8]]) {
   set(value);exports[name]();
   const stored=width===2?view.getInt16(0x1000,true):width===4?view.getInt32(0x1000,true):view.getBigInt64(0x1000,true);
   assert.equal(stored,width===8?BigInt(expected):expected||0,`${name} ${bits} ${value}`);
  }
 }
}
// Complete's radius=12 cache count: floor(12/3-.5), then trunc(+.5).
mode(0x173f);set(3.5);exports.round();assert.equal(view.getFloat64(0x2400,true),3);
mode(0x0e7f);set(3.5);exports.dword();assert.equal(view.getInt32(0x1000,true),3);
mode(0x027f);set(32767.5);exports.word();assert.equal(view.getInt16(0x1000,true),-32768);assert.equal(env.fp_status.value&1,1);
mode(0x067f);set(32767.5);exports.word();assert.equal(view.getInt16(0x1000,true),32767);
for(const [save,restore] of [['save','restore'],['save_env','load_env']]) {
 mode(0x1b3f);env.fp_top.value=3;env.fp_status.value=0x425;
 exports[save]();assert.equal(view.getUint16(0x1100,true),0x1b3f);
 assert.equal(view.getUint16(0x1104,true),0x1c25);
 exports.init();assert.equal(exports.fpuControl.value,0x027f);
 exports[restore]();assert.equal(exports.fpuControl.value,0x1b3f);
 assert.equal(env.fp_top.value,3);assert.equal(env.fp_status.value,0x425);
}
assert.equal(new WebAssembly.Instance(module,{env}).exports.fpuControl.value,0x027f,'control word is per instance/thread');
"#,
    );
}

#[test]
fn fused_rounding_uses_the_active_control_word() {
    use crate::{guestmem::GuestMem, x86::decode::decode_insn};
    let mut memory = GuestMem::new();
    memory.write(0, &[0xd9, 0xfc].repeat(6));
    let instructions: Vec<_> = (0..6)
        .map(|i| decode_insn(&memory, 0, i * 2).unwrap())
        .collect();
    let run = super::runs::emit(&instructions.iter().collect::<Vec<_>>()).unwrap();
    crate::translate::test_support::execute_sequences_v8(
        &[("round", run.code)],
        r#"
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
const env={memory:new WebAssembly.Memory({initial:1}),observe(){}};
for(const name of ['eax','ecx','edx','ebx','esp','ebp','esi','edi','flags','fp_top','fp_status','trap_va','call_target','resume_next','actual_return','teb_base'])
 env[name]=new WebAssembly.Global({value:'i32',mutable:true},0);
const {exports}=new WebAssembly.Instance(new WebAssembly.Module(readFileSync(0)),{env});
const view=new DataView(env.memory.buffer);env.teb_base.value=0x2000;
for(const [word,input,expected] of [[0x027f,3.5,4],[0x067f,3.5,3],[0x0a7f,-3.5,-3],[0x0e7f,3.5,3]]) {
 exports.fpuControl.value=word;view.setFloat64(0x2400,input,true);exports.round();
 assert.equal(view.getFloat64(0x2400,true),expected);
}
"#,
    );
}
