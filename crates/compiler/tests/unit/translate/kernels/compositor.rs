use super::*;
use crate::{
    guestmem::GuestMem, translate::test_support::execute_sequences_v8, x86::decode::decode_insn,
};
use std::collections::{BTreeMap, HashMap};

// Straight-line instruction fixtures; no game data or executable is required.
const FULL: &str = "668b820c5c6e006a1f66c1f8030fbff00fb7824c5c6e000fb78a0c5d6e008bd8c1eb05d1ebc1e60523df8975e803de8bf1c1ee05d1eec1e30523f7660fb6b4335c5e6e008bd8c1eb0b23df8bf9035de8c1ef0bc1e30583e71f660fb6bc3b5c5e6e00c1e7050bf75f23c723cf0345e8c1e606c1e005660fb684085c5e6e000bf0";
const TAIL: &str = "8b45f88d0c00668b810c5c6e0066c1f8030fbfd00fb7814c5c6e000fb7890c5d6e008bf08bd9c1ee05d1eec1e20523f78955d8c1eb0503f2d1ebc1e60523df660fb6b41e5c5e6e008bd823c7c1eb0b23df0345d803da8bd1c1ea0bc1e30523d723cfc1e005660fb694135c5e6e00660fb684085c5e6e00c1e2050bf2c1e6060bf0";

#[test]
fn fused_pixels_preserve_registers_flags_memory_aliases_and_partial_faults() {
    let functions = HashMap::new();
    let imports = HashMap::new();
    let context = super::super::super::emit::EmitCtx {
        memory_minimum_pages: 1,
        id: BTreeMap::new(),
        func_idx_of: &functions,
        idx_of: 0,
        iat_shim: &imports,
        block_trace: None,
    };
    let mut routines = Vec::new();
    for (plain, fast, address, text) in [
        ("full_plain", "full_fast", 0x499c44, FULL),
        ("tail_plain", "tail_fast", 0x499ec0, TAIL),
    ] {
        let bytes: Vec<_> = (0..text.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&text[i..i + 2], 16).unwrap())
            .collect();
        let mut memory = GuestMem::new();
        memory.write(0, &bytes);
        let mut decoded = Vec::new();
        let mut offset = 0;
        while offset < bytes.len() as u32 {
            let mut instruction = decode_insn(&memory, 0, offset).unwrap();
            offset += instruction.size;
            instruction.va += address;
            decoded.push(instruction);
        }
        let references: Vec<_> = decoded.iter().collect();
        let run = emit(&references, &context).unwrap();
        assert_eq!(run.count, decoded.len());
        routines.push((
            plain,
            decoded
                .iter()
                .flat_map(|i| emit_insn(i, &context, -1, 0).unwrap())
                .collect(),
        ));
        routines.push((fast, run.code));
        assert!(
            emit(&references[..references.len() - 1], &context).is_none(),
            "cannot cross a segment/checkpoint"
        );
    }
    execute_sequences_v8(
        &routines,
        r#"
import assert from 'node:assert/strict';import {readFileSync} from 'node:fs';
const names=['eax','ecx','edx','ebx','esp','ebp','esi','edi','flags','fp_top','fp_status','trap_va','call_target','resume_next','actual_return','teb_base'];
const env={memory:new WebAssembly.Memory({initial:256}),observe(){}};
for(const name of names)env[name]=new WebAssembly.Global({value:'i32',mutable:true},0);
const {exports}=new WebAssembly.Instance(new WebAssembly.Module(readFileSync(0)),{env});
const bytes=new Uint8Array(env.memory.buffer),view=new DataView(env.memory.buffer);
const alpha=0x6e5c0c,source=0x6e5c4c,previous=0x6e5d0c,table=0x6e5e5c,end=table+32768;
const valid=(at,n)=>at>=0&&at+n<=bytes.length;
const tableBytes=Uint8Array.from({length:32768},(_,i)=>(i*73+(i>>>7)*19)&255);
let seed=19;const random=()=>seed=(Math.imul(seed,1664525)+1013904223)|0;
function compare(kind,options){
 const {index=0,weight=128,left=0x1234,right=0xabcd,mask=31,frame=0x18000,stack=0x17fc0}=options;
 const weightSlot=(frame-(kind==='tail'?40:24))>>>0,pushed=(stack-4)>>>0;
 const initial=names.map(()=>random());
 function run(suffix){
  bytes.fill(0xa5,alpha,end);bytes.set(tableBytes,table);
  for(const at of [weightSlot,pushed])if(valid(at,4))view.setUint32(at,0xa5a5a5a5,true);
  if(valid(frame-8,4))view.setUint32(frame-8,index,true);
  for(const [at,value] of [[alpha+2*index,weight],[source+2*index,left],[previous+2*index,right]])
   if(valid(at,2))view.setUint16(at,value,true);
  names.forEach((name,i)=>env[name].value=initial[i]);
  env.edx.value=2*index;env.ebp.value=frame;env.esp.value=stack;env.edi.value=mask;
  let trapped=false;try{exports[kind+'_'+suffix]();}catch(e){assert.ok(e instanceof WebAssembly.RuntimeError);trapped=true;}
  return {trapped,registers:names.map(name=>env[name].value),data:bytes.slice(alpha,end),
   stores:[weightSlot,pushed].map(at=>valid(at,4)?bytes.slice(at,at+4):null)};
 }
 assert.deepEqual(run('fast'),run('plain'),kind+' '+JSON.stringify(options));
}
for(const kind of ['full','tail']){
 for(let i=0;i<512;i++)compare(kind,{index:i%32,weight:i%256,left:random()&65535,right:random()&65535});
 for(const weight of [256,0x8000,0xffff])compare(kind,{weight});
 for(const mask of [0,7,0xffff,-1])compare(kind,{mask});
 for(const index of [32,33,0x10000000])compare(kind,{index});
 for(const stack of [0,2,bytes.length+4,alpha+4,source+4,previous+4,table+4,0x18000-(kind==='tail'?40:24)+4])compare(kind,{stack});
 for(const frame of [0,8,bytes.length+64,alpha+(kind==='tail'?40:24),source+(kind==='tail'?40:24),table+(kind==='tail'?40:24)])compare(kind,{frame});
}
"#,
    );
}

fn span_routines() -> Vec<(&'static str, Vec<u8>)> {
    use crate::translate::{layout::NEXT, scheduling::CHECKPOINT_BUDGET_GLOBAL};

    let functions = HashMap::new();
    let imports = HashMap::new();
    let context = super::super::super::emit::EmitCtx {
        memory_minimum_pages: 1,
        id: BTreeMap::new(),
        func_idx_of: &functions,
        idx_of: 0,
        iat_shim: &imports,
        block_trace: None,
    };
    // Original instructions after the blend prefix, ending immediately before
    // the loop's signed conditional branch. No game executable is required.
    let suffix = "8b451883451802424266893083fa40";
    let mut routines = Vec::new();
    for (name, optimized) in [("plain", false), ("span", true)] {
        let mut code = set_reg(abi::TRAP_VA, i32c(0));
        code.extend(i32c(0));
        code.extend(local_set(NEXT));
        code.extend([Op::BLOCK, 0x40, Op::LOOP, 0x40]);
        code.extend(local_get(NEXT));
        code.extend([Op::BR_IF, 1]);
        code.extend(set_reg(abi::TRAP_VA, i32c(0x499c44)));
        code.extend(set_reg(
            CHECKPOINT_BUDGET_GLOBAL,
            binary(reg(CHECKPOINT_BUDGET_GLOBAL), i32c(1), Op::I32_SUB),
        ));
        code.extend(reg(CHECKPOINT_BUDGET_GLOBAL));
        code.extend([Op::I32_EQZ, Op::IF, 0x40]);
        code.extend(set_reg(CHECKPOINT_BUDGET_GLOBAL, i32c(256)));
        code.extend([Op::CALL, 0, Op::END]); // observe the complete guest state
        if optimized {
            code.extend(full_span(1, 0, true));
        }
        // The retained scalar fallback includes the already-shipped prefix
        // fusion, so this compares the span against the current compiler.
        let mut bytes: Vec<_> = (0..FULL.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&FULL[i..i + 2], 16).unwrap())
            .collect();
        let prefix_bytes = bytes.len() as u32;
        bytes.extend(
            (0..suffix.len())
                .step_by(2)
                .map(|i| u8::from_str_radix(&suffix[i..i + 2], 16).unwrap()),
        );
        let mut memory = GuestMem::new();
        memory.write(0, &bytes);
        let mut decoded = Vec::new();
        let mut at = 0;
        while at < bytes.len() as u32 {
            let mut instruction = decode_insn(&memory, 0, at).unwrap();
            at += instruction.size;
            instruction.va += 0x499c44;
            decoded.push(instruction);
        }
        let references: Vec<_> = decoded
            .iter()
            .take_while(|i| i.va < 0x499c44 + prefix_bytes)
            .collect();
        code.extend(emit(&references, &context).unwrap().code);
        for instruction in decoded.iter().skip(references.len()) {
            code.extend(emit_insn(instruction, &context, -1, 0).unwrap());
        }
        code.extend(binary(reg(abi::EDX), i32c(64), Op::I32_LT_S));
        code.extend([Op::BR_IF, 0, Op::END, Op::END]);
        routines.push((name, code));
    }
    routines.push((
        "set_budget",
        set_reg(CHECKPOINT_BUDGET_GLOBAL, reg(abi::EAX)),
    ));
    routines.push((
        "read_budget",
        set_reg(abi::EAX, reg(CHECKPOINT_BUDGET_GLOBAL)),
    ));
    routines
        .into_iter()
        .map(|(name, code)| (name, crate::translate::optimize::promote(&code).0))
        .collect()
}

#[test]
fn blended_spans_preserve_checkpoints_faults_aliases_and_registers() {
    execute_sequences_v8(
        &span_routines(),
        include_str!("../../../fixtures/compositor-span.mjs"),
    );
}

// SIMS_COMPOSITOR_BENCH_MODULE=.tools/compositor-span.wasm cargo test -p sims-compiler --lib export_compositor_span_benchmark -- --ignored
#[test]
#[ignore = "exports an isolated benchmark module on explicit request"]
fn export_compositor_span_benchmark() {
    use crate::wasm::builder::{FuncDef, WasmBuilder};
    let destination = std::env::var("SIMS_COMPOSITOR_BENCH_MODULE")
        .expect("set SIMS_COMPOSITOR_BENCH_MODULE to an output path");
    let mut builder = WasmBuilder::new();
    let signature = builder.add_type(&[], &[]);
    builder.import_memory(1, None);
    for name in abi::REG_GLOBALS {
        builder.add_imported_global(name);
    }
    builder.add_private_i32_global(256);
    builder.add_private_i32_global(crate::translate::emit_fpu::CONTROL_WORD);
    builder.add_import("env", "observe", signature);
    let mut benchmarks = Vec::new();
    for (name, body) in span_routines() {
        let index = builder.add_func(FuncDef {
            type_idx: signature,
            locals: crate::translate::layout::locals_list(),
            body,
        });
        builder.export_func(name, index);
        match name {
            "plain" => benchmarks.push(("bench_plain", index)),
            "span" => benchmarks.push(("bench_span", index)),
            _ => {}
        }
    }
    let counted = builder.add_type(&[0x7f], &[]);
    for (name, index) in benchmarks {
        let mut body = vec![Op::BLOCK, 0x40, Op::LOOP, 0x40];
        body.extend(local_get(0));
        body.extend([Op::I32_EQZ, Op::BR_IF, 1]);
        body.extend(set_reg(abi::EDX, i32c(0)));
        body.extend(store(
            binary(reg(abi::EBP), i32c(24), Op::I32_ADD),
            i32c(0x20000),
        ));
        body.push(Op::CALL);
        body.extend(crate::wasm::leb::uleb(index));
        body.extend(binary(local_get(0), i32c(1), Op::I32_SUB));
        body.extend(local_set(0));
        body.extend([Op::BR, 0, Op::END, Op::END]);
        let index = builder.add_func(FuncDef {
            type_idx: counted,
            locals: Vec::new(),
            body,
        });
        builder.export_func(name, index);
    }
    let wasm = builder.build();
    wasmparser::Validator::new().validate_all(&wasm).unwrap();
    std::fs::write(destination, wasm).unwrap();
}
