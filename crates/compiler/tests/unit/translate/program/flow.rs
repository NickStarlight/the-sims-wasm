use super::*;
use crate::translate::{
    emit::emit_insn,
    emit_helpers::{local_get, set_reg},
    test_support::execute_sequences_v8,
};
use crate::x86::types::{FlagFx, Rep};
use std::collections::HashMap;

const COUNT: usize = 160;

// Compare the old dispatcher-only shape with forward-edge lowering over a
// graph containing loops, joins and targets beyond one-byte label depths.
fn graph(direct: bool) -> Seq {
    let functions = HashMap::new();
    let imports = HashMap::new();
    let context = EmitCtx {
        id: (0..COUNT).map(|i| (i as u32, i as i32)).collect(),
        func_idx_of: &functions,
        idx_of: 0,
        iat_shim: &imports,
        block_trace: Some(0),
    };
    let mut out = reg(abi::RESUME_NEXT);
    out.extend(local_set(NEXT));
    out.extend(set_reg(abi::RESUME_NEXT, i32c(0)));
    out.extend([Op::BLOCK, 0x40, Op::LOOP, 0x40]);
    for _ in 0..COUNT {
        out.extend([Op::BLOCK, 0x40]);
    }
    out.extend(local_get(NEXT));
    out.push(Op::BR_TABLE);
    out.extend(uleb(COUNT as u32));
    for i in 0..COUNT {
        out.extend(uleb(if direct { i } else { COUNT - i - 1 } as u32));
    }
    out.extend(uleb(COUNT as u32 + 1));
    for position in 0..COUNT {
        let i = if direct {
            position
        } else {
            COUNT - position - 1
        };
        out.push(Op::END);
        out.extend(set_reg(abi::TRAP_VA, i32c(i as i32)));
        out.extend([Op::CALL, 0]);
        let mut sum = reg(abi::EAX);
        sum.extend(i32c(i as i32 + 1));
        sum.push(Op::I32_ADD);
        out.extend(set_reg(abi::EAX, sum));
        // Record memory as well as final registers and checkpoint snapshots.
        out.extend(i32c(100 + i as i32 * 4));
        out.extend(reg(abi::EAX));
        out.extend([Op::I32_STORE, 2, 0]);
        let (mnem, target, successor) = match i {
            0 => (Mnem::Jcxz, COUNT as u32 - 1, 1),
            1 => {
                let mut value = reg(abi::ECX);
                value.extend(i32c(-1));
                value.push(Op::I32_ADD);
                out.extend(set_reg(abi::ECX, value));
                // Forward taken edge, backward untaken edge.
                (Mnem::Jcxz, 2, 0)
            }
            2 | 3 => {
                out.extend(set_reg(abi::FLAGS, reg(abi::EDX)));
                // Unresolved Jcc must choose its successor on either result.
                (
                    Mnem::Jcc,
                    if i == 2 { 150 } else { 0xffff0000 },
                    i as i32 + 1,
                )
            }
            50 => (Mnem::Jmp, 130, -1),
            i if i + 1 == COUNT => {
                out.push(Op::RETURN);
                continue;
            }
            _ => (Mnem::Jmp, i as u32 + 1, -1),
        };
        let insn = Insn {
            va: i as u32,
            size: 2,
            mnem,
            ops: Vec::new(),
            cc: Some(2), // JB: tests the carry bit written above.
            rel: Some(target),
            rep: Rep::None,
            flags: FlagFx::None,
            switch_targets: Vec::new(),
        };
        if direct {
            let flow = ForwardFlow::new(i, COUNT);
            out.extend(flow.instruction(&insn, &context, successor).unwrap());
        } else {
            out.extend(emit_insn(&insn, &context, successor, i as i32).unwrap());
        }
    }
    out.extend([Op::BR, 1, Op::END, Op::END]);
    out
}

#[test]
fn forward_edges_preserve_all_resume_entries_loops_memory_and_checkpoint_state() {
    execute_sequences_v8(
        &[("dispatch", graph(false)), ("forward", graph(true))],
        r#"
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
const module = new WebAssembly.Module(readFileSync(0));
const names = ['eax','ecx','edx','ebx','esp','ebp','esi','edi','flags','fp_top','fp_status','trap_va','call_target','resume_next','actual_return','teb_base'];
for (const resume of [-1, ...Array.from({length:160}, (_,i)=>i),160,0x7fffffff]) {
 for (const count of [1,3]) for (const choice of [0,1]) {
  const results=[];
  for (const routine of ['dispatch','forward']) {
   const checkpoints=[];
   const env={memory:new WebAssembly.Memory({initial:1}), observe(){
    if(checkpoints.length>1000)throw new Error('Unexpected infinite control-flow loop');
    checkpoints.push(names.map(name=>env[name].value));
   }};
   for(const [i,name] of names.entries())env[name]=new WebAssembly.Global({value:'i32',mutable:true},i);
   env.eax.value=0;env.ecx.value=count;env.edx.value=choice;env.resume_next.value=resume;
   new WebAssembly.Instance(module,{env}).exports[routine]();
   results.push({checkpoints,registers:names.map(name=>env[name].value),memory:[...new Uint8Array(env.memory.buffer)]});
  }
  assert.deepEqual(results[1],results[0],'resume='+resume+', count='+count+', choice='+choice);
 }
}
"#,
    );
}
