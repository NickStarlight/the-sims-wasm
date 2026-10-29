use crate::translate::{
    emit::{emit_insn, EmitCtx},
    test_support::execute_sequences_v8,
};
use crate::x86::types::{Insn, Mnem, Operand, Rep};
use std::collections::{BTreeMap, HashMap};

#[test]
fn repeated_store_matches_directions_widths_boundaries_and_partial_faults() {
    let functions = HashMap::new();
    let imports = HashMap::new();
    let context = EmitCtx {
        id: BTreeMap::new(),
        func_idx_of: &functions,
        idx_of: 0,
        iat_shim: &imports,
        block_trace: None,
    };
    let mut routines = Vec::new();
    for (name, size) in [("byte", 1), ("word", 2), ("dword", 4)] {
        let instruction = Insn {
            mnem: Mnem::Stos,
            ops: vec![Operand::Imm(size)],
            rep: Rep::WhileEqual,
            ..Default::default()
        };
        routines.push((name, emit_insn(&instruction, &context, -1, 2).unwrap()));
    }
    execute_sequences_v8(
        &routines,
        r#"
import assert from 'node:assert/strict';import {readFileSync} from 'node:fs';
const names=['eax','ecx','edx','ebx','esp','ebp','esi','edi','flags','fp_top','fp_status','trap_va','call_target','resume_next','actual_return','teb_base'];
const env={memory:new WebAssembly.Memory({initial:1}),observe(){}};
for(const name of names)env[name]=new WebAssembly.Global({value:'i32',mutable:true},0);
const {exports:run}=new WebAssembly.Instance(new WebAssembly.Module(readFileSync(0)),{env});
const actual=new Uint8Array(env.memory.buffer);
for(const [name,size] of [['byte',1],['word',2],['dword',4]]) {
 for(const backwards of [false,true])for(const count of [0,1,7,8,9,15,16,127,1023,1024,1025,2048,4096]) {
  actual.fill(0xaa);const expected=actual.slice();
  const start=backwards?64+count*size:64;
  const flags=0x8d5|(backwards?0x400:0);
  env.eax.value=0x12345678;env.ecx.value=count;env.edi.value=start;env.flags.value=flags;
  for(let i=0;i<count;i++)for(let b=0;b<size;b++)expected[start+(backwards?-1:1)*i*size+b]=(0x12345678>>>(b*8))&255;
  run[name]();assert.deepEqual(actual,expected);
  assert.equal(env.ecx.value,0);assert.equal(env.edi.value,start+(backwards?-1:1)*count*size);assert.equal(env.flags.value,flags);
 }
 actual.fill(0xaa);env.eax.value=0x12345678;env.ecx.value=64;env.edi.value=65536-20;env.flags.value=0;
 assert.throws(()=>run[name](),WebAssembly.RuntimeError);
 assert.equal(env.ecx.value,64-20/size);assert.equal(env.edi.value,65536);
 actual.fill(0xaa);env.ecx.value=64;env.edi.value=0;env.flags.value=0x400;
 assert.throws(()=>run[name](),WebAssembly.RuntimeError);
 assert.equal(env.ecx.value,63);assert.equal(env.edi.value,-size);
}
"#,
    );
}
