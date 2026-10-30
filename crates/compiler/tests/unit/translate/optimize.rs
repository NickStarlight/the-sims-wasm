use super::*;
use crate::translate::{emit_helpers::i32c, test_support::execute_sequences_v8};
use crate::wasm::opcodes::Op;

#[test]
fn promotion_publishes_state_at_calls_control_edges_memory_and_arithmetic_faults() {
    let mut code = Vec::new();
    for _ in 0..3 {
        code.extend(reg(0));
        code.extend(i32c(17));
        code.push(Op::I32_ADD);
        code.extend([Op::GLOBAL_SET, 0]);
    }
    code.extend([Op::CALL, 0]);
    code.extend(reg(0));
    code.extend([Op::IF, 0x40]);
    code.extend(set_reg(1, reg(0)));
    code.extend([Op::ELSE]);
    code.extend(set_reg(1, i32c(99)));
    code.push(Op::END);
    code.extend(i32c(100));
    code.extend(reg(1));
    code.extend([Op::I32_STORE, 2, 0]);
    for _ in 0..3 {
        code.extend(set_reg(2, reg(1)));
    }
    code.extend(reg(2));
    code.extend(i32c(0));
    code.push(0x6d); // i32.div_s traps
    code.push(0x1a);
    let (optimized, offsets) = promote(&code);
    assert!(offsets.windows(2).all(|pair| pair[0].0 < pair[1].0));
    execute_sequences_v8(
        &[("plain", code), ("optimized", optimized)],
        r#"
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
const wasm=readFileSync(0), names=['eax','ecx','edx','ebx','esp','ebp','esi','edi','flags','fp_top','fp_status','trap_va','call_target','resume_next','actual_return','teb_base'];
for (const value of [-51,0,1,0x7fffffff,-2147483648]) {
 const outcomes=[];
 for(const routine of ['plain','optimized']) {
  const env={memory:new WebAssembly.Memory({initial:1})};
  for(const [i,name] of names.entries())env[name]=new WebAssembly.Global({value:'i32',mutable:true},i);
  const observations=[];
  env.observe=()=>observations.push(names.map(name=>env[name].value));
  const {exports}=new WebAssembly.Instance(new WebAssembly.Module(wasm),{env});
  env.eax.value=value;
  assert.throws(()=>exports[routine](),WebAssembly.RuntimeError);
  outcomes.push({observations,registers:names.map(name=>env[name].value),memory:[...new Uint8Array(env.memory.buffer,100,4)]});
 }
 assert.deepEqual(outcomes[1],outcomes[0]);
}

"#,
    );
}

#[test]
fn promotion_keeps_clean_registers_across_memory_and_publishes_each_fault_boundary() {
    let mut code = Vec::new();
    for offset in [0, 4, 8, 12] {
        code.extend(set_reg(0, {
            let mut value = reg(0);
            value.extend(i32c(17));
            value.push(Op::I32_ADD);
            value
        }));
        code.extend(reg(5));
        code.extend(i32c(offset));
        code.push(Op::I32_ADD);
        code.extend([Op::I32_LOAD, 2, 0]);
        code.extend([Op::GLOBAL_SET, 1]);
        code.extend(reg(6));
        code.extend(i32c(offset));
        code.push(Op::I32_ADD);
        code.extend(reg(1));
        code.extend([Op::I32_STORE, 2, 0]);
    }
    code.extend([Op::CALL, 0]);
    // The observer changes registers and grows/replaces the memory buffer.
    // No cached value may survive that call, even for read-only registers.
    for _ in 0..3 {
        code.extend(set_reg(0, reg(5)));
        code.extend(set_reg(1, reg(6)));
    }
    code.extend(reg(5));
    code.extend([Op::I32_LOAD, 2, 0, Op::GLOBAL_SET, 2]);
    let (optimized, _) = promote(&code);
    execute_sequences_v8(
        &[("plain", code), ("optimized", optimized)],
        r#"
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
const module=new WebAssembly.Module(readFileSync(0));
const names=['eax','ecx','edx','ebx','esp','ebp','esi','edi','flags','fp_top','fp_status','trap_va','call_target','resume_next','actual_return','teb_base'];
let cases=0;
for(const source of [64,0,65520,65524,65528,65532,65533,65536,0xfffffff8]) {
 for(const destination of [128,64,65,65520,65524,65528,65532,65535,65536,0xfffffffc]) {
  for(const stop of [false,true]) {
   const results=[];
   for(const routine of ['plain','optimized']) {
    const observations=[];
    const env={memory:new WebAssembly.Memory({initial:1,maximum:2}),observe(){
     observations.push(names.map(n=>env[n].value));
     if(stop)throw new Error('observer stop');
     env.memory.grow(1);env.ebp.value=65536;env.esi.value=65540;
     new DataView(env.memory.buffer).setUint32(65536,0xaabbccdd,true);
    }};
    for(const [i,name] of names.entries())env[name]=new WebAssembly.Global({value:'i32',mutable:true},i*317);
    new Uint8Array(env.memory.buffer).fill(0xa5);
    env.ebp.value=source;env.esi.value=destination;
    const {exports}=new WebAssembly.Instance(module,{env});
    let error;
    try{exports[routine]()}catch(e){error=e instanceof WebAssembly.RuntimeError?'memory fault':e.message}
    results.push({registers:names.map(n=>env[n].value),memory:new Uint8Array(env.memory.buffer).slice(),observations,error});
   }
   assert.deepEqual(results[1],results[0],JSON.stringify({source,destination,stop}));cases++;
  }
 }
}
assert.equal(cases,180);
"#,
    );
}
