use super::*;
use crate::{
    guestmem::GuestMem, translate::test_support::execute_sequences_v8, x86::decode::decode_insn,
};

fn instructions(bytes: &[u8]) -> Vec<Insn> {
    let mut memory = GuestMem::new();
    memory.write(0, bytes);
    let mut decoded = Vec::new();
    let mut offset = 0;
    while offset < bytes.len() as u32 {
        let instruction = decode_insn(&memory, 0, offset).unwrap();
        offset += instruction.size;
        decoded.push(instruction);
    }
    decoded
}

// The generic seven-deep scheduling used by an x87 128-byte memcpy block.
const BLOCK: &[u8] = &[
    0xdf, 0x2a, 0xdf, 0x6a, 0x20, 0xdf, 0x6a, 0x40, 0xdf, 0x6a, 0x60, 0xd9, 0xcb, 0xdf, 0x6a, 0x10,
    0xdf, 0x6a, 0x08, 0xdf, 0x6a, 0x18, 0xd9, 0xcb, 0xdf, 0x38, 0xdf, 0x78, 0x08, 0xdf, 0x78, 0x10,
    0xdf, 0x78, 0x18, 0xd9, 0xc9, 0xdf, 0x6a, 0x30, 0xdf, 0x6a, 0x28, 0xdf, 0x6a, 0x38, 0xd9, 0xcb,
    0xdf, 0x78, 0x20, 0xdf, 0x78, 0x28, 0xdf, 0x78, 0x30, 0xdf, 0x78, 0x38, 0xdf, 0x6a, 0x50, 0xdf,
    0x6a, 0x48, 0xdf, 0x6a, 0x58, 0xd9, 0xcb, 0xdf, 0x78, 0x40, 0xdf, 0x78, 0x48, 0xdf, 0x78, 0x50,
    0xdf, 0x78, 0x58, 0xdf, 0x6a, 0x70, 0xdf, 0x6a, 0x68, 0xdf, 0x6a, 0x78, 0xd9, 0xcb, 0xdf, 0x78,
    0x60, 0xdf, 0x78, 0x68, 0xdf, 0x78, 0x70, 0xdf, 0x78, 0x78,
];

#[test]
fn integer_copy_preserves_bits_order_control_and_live_stack() {
    let mut routines = Vec::new();
    for (name, bytes) in [("pair", &[0xdf, 0x2a, 0xdf, 0x38][..]), ("block", BLOCK)] {
        let decoded = instructions(bytes);
        let run = emit(&decoded.iter().collect::<Vec<_>>()).unwrap();
        assert_eq!(run.count, decoded.len());
        routines.push((name, run.code));
    }
    execute_sequences_v8(
        &routines,
        r#"
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
const env={memory:new WebAssembly.Memory({initial:1}),observe(){}};
const names=['eax','ecx','edx','ebx','esp','ebp','esi','edi','flags','fp_top','fp_status','trap_va','call_target','resume_next','actual_return','teb_base'];
for(const name of names)env[name]=new WebAssembly.Global({value:'i32',mutable:true},0);
const {exports}=new WebAssembly.Instance(new WebAssembly.Module(readFileSync(0)),{env});
const bytes=new Uint8Array(env.memory.buffer),v=new DataView(env.memory.buffer);
const values=[0n,1n,-1n,2n**53n+1n,-(2n**53n+1n),2n**63n-1n,-(2n**63n),
 0x7ff0000000000001n,0x7ff8000000000001n,0x0123456789abcdefn,-0x123456789abcdefn];
const read=(array,at)=>new DataView(array.buffer,array.byteOffset,array.byteLength).getBigInt64(at,true);
const write=(array,at,x)=>new DataView(array.buffer,array.byteOffset,array.byteLength).setBigInt64(at,x,true);
for(const top of [0,3,7])for(const cw of [0x027f,0x173f,0x1b3f,0x0e7f]) {
 for(const name of names)env[name].value=0x12340000;
 env.teb_base.value=0x2000;env.fp_top.value=top;env.fp_status.value=0x4301;
 exports.fpuControl.value=cw;
 v.setFloat64(0x2400+8*top,Math.PI,true);
 for(const value of values) {
  env.edx.value=0x1000;env.eax.value=0x1100;v.setBigInt64(0x1000,value,true);
  const saved=names.map(name=>env[name].value);
  exports.pair();assert.equal(v.getBigInt64(0x1100,true),value);
  assert.deepEqual(names.map(name=>env[name].value),saved);
  assert.equal(exports.fpuControl.value,cw);assert.equal(v.getFloat64(0x2400+8*top,true),Math.PI);
 }
 // Compare overlap with the actual load/store schedule, not memmove semantics.
 for(const delta of [-48,-1,0,1,8,32,128]) {
  const source=0x1000,destination=source+delta;
  for(let i=0;i<32;i++)v.setBigInt64(0xf80+8*i,values[i%values.length],true);
  const expected=bytes.slice();const stack=[];
  const load=offset=>stack.unshift(read(expected,source+offset));
  const store=offset=>write(expected,destination+offset,stack.shift());
  const swap=i=>{[stack[0],stack[i]]=[stack[i],stack[0]];};
  load(0);load(32);load(64);load(96);swap(3);load(16);load(8);load(24);swap(3);
  store(0);store(8);store(16);store(24);swap(1);load(48);load(40);load(56);swap(3);
  store(32);store(40);store(48);store(56);load(80);load(72);load(88);swap(3);
  store(64);store(72);store(80);store(88);load(112);load(104);load(120);swap(3);
  store(96);store(104);store(112);store(120);assert.equal(stack.length,0);
  env.edx.value=source;env.eax.value=destination;
  const saved=names.map(name=>env[name].value);
  exports.block();assert.deepEqual(bytes.slice(0xf80,0x1180),expected.slice(0xf80,0x1180),`overlap ${delta}`);
  assert.deepEqual(names.map(name=>env[name].value),saved);
  assert.equal(exports.fpuControl.value,cw);assert.equal(v.getFloat64(0x2400+8*top,true),Math.PI);
 }
}
"#,
    );
}

#[test]
fn integer_copy_requires_balanced_values_without_arithmetic_or_incoming_stack_access() {
    for bytes in [
        vec![0xdf, 0x2a],                         // Unconsumed integer value.
        vec![0xdf, 0x38],                         // Store of an incoming floating value.
        vec![0xdf, 0x2a, 0xd9, 0xc9, 0xdf, 0x38], // FXCH reaches incoming ST1.
        vec![0xdf, 0x2a, 0xd9, 0xe0, 0xdf, 0x38], // Numeric negation.
        vec![0xdf, 0x2a, 0xd9, 0xfc, 0xdf, 0x38], // Numeric rounding.
        [[0xdf, 0x2a].repeat(9), [0xdf, 0x38].repeat(9)].concat(),
    ] {
        let decoded = instructions(&bytes);
        assert!(emit(&decoded.iter().collect::<Vec<_>>()).is_none());
    }
    let decoded = instructions(&[0xdf, 0x2a, 0xdf, 0x38, 0xdf, 0x2a, 0xd9, 0xe0]);
    assert_eq!(emit(&decoded.iter().collect::<Vec<_>>()).unwrap().count, 2);
}
