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
