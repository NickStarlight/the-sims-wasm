//! Exercise dispatch labels beyond one-byte LEBs and every resumable entry.
use super::*;
use crate::pe::PeSection;
use crate::translate::abi;
use std::{
    io::Write,
    process::{Command, Stdio},
};

#[test]
fn jump_table_visits_and_resumes_all_segments_and_rejects_invalid_indices() {
    const COUNT: u32 = 160;
    const BASE: u32 = 0x401000;
    // Interleave the first and last halves, exercising both forward and backward jumps.
    let order: Vec<u32> = (0..COUNT / 2).flat_map(|i| [i, COUNT - 1 - i]).collect();
    let mut image = GuestMem::new();
    for (position, &index) in order.iter().enumerate() {
        let address = BASE + index * 8;
        let mut code = vec![0x40]; // inc eax
        if let Some(&next) = order.get(position + 1) {
            code.push(0xe9);
            code.extend((BASE + next * 8).wrapping_sub(address + 6).to_le_bytes());
        } else {
            code.push(0xc3);
        }
        image.write(address as usize, &code);
    }
    // A high-address entry switches to lower-address cases through an extra IF
    // scope in the emitter. Exercise those label depths and nonzero entry IDs.
    image.write(0x402000, &[0xba, 111, 0, 0, 0, 0xc3]);
    image.write(0x402020, &[0xba, 222, 0, 0, 0, 0xc3]);
    image.write(0x402100, &[0xff, 0x24, 0x85, 0, 0x30, 0x40, 0]);
    image.write_u32(0x403000, 0x402000);
    image.write_u32(0x403004, 0x402020);
    // A branch uses the same table with either nonnegative indices or negative
    // indices relative to its end, as optimized CRT copy loops do.
    image.write(
        0x404000,
        &[
            0x85, 0xc9, 0x78, 7, // TEST ECX,ECX; JS negative
            0xff, 0x24, 0x8d, 0x80, 0x40, 0x40, 0, // JMP [ECX*4+0x404080]
            0xff, 0x24, 0x8d, 0x88, 0x40, 0x40, 0, // JMP [ECX*4+0x404088]
        ],
    );
    image.write(0x404040, &[0xba, 11, 0, 0, 0, 0xc3]);
    image.write(0x404060, &[0xba, 22, 0, 0, 0, 0xc3]);
    image.write_u32(0x404080, 0x404040);
    image.write_u32(0x404084, 0x404060);
    let info = crate::pe::PeInfo {
        name: None,
        is_dll: false,
        exports: vec![],
        entry_va: BASE,
        image_base: 0x400000,
        size_of_image: 0x5000,
        size_of_headers: 0,
        reloc_rva: 0,
        reloc_size: 0,
        imports: vec![],
        sections: vec![PeSection {
            name: ".text".into(),
            rva: 0x1000,
            virtual_size: 0x3100,
            file_offset: 0,
            size_of_raw_data: 0x3100,
            characteristics: 0x60000020,
            va: BASE,
        }],
    };
    let program = translate_program(&image, 0, BASE, &info, &[0x402100, 0x404000], &[]).unwrap();
    assert_eq!(program.func_addrs, [BASE, 0x402100, 0x404000]);
    assert_eq!(program.blocks_by_func[0].1.len(), COUNT as usize);
    assert!(program.diagnostics.is_empty());
    wasmparser::Validator::new()
        .validate_all(&program.wasm)
        .unwrap();
    let input = serde_json::json!({ "wasm": program.wasm, "registers": abi::REG_GLOBALS,
        "order": order, "base": BASE });
    let mut child = Command::new("node")
        .args([
            "--input-type=module",
            "-e",
            r#"
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
const {wasm, registers, order, base} = JSON.parse(readFileSync(0, 'utf8'));
const module = new WebAssembly.Module(Uint8Array.from(wasm));
assert.equal(WebAssembly.Module.customSections(module, 'name').length, 1);
const env = {memory:new WebAssembly.Memory({initial:256,maximum:65536}),
 table:new WebAssembly.Table({element:'anyfunc',initial:300000}),
 dispatch(){assert.fail('unexpected import')},blockTrace(){}};
for(const name of registers) env[name]=new WebAssembly.Global({value:'i32',mutable:true},0);
const {exports}=new WebAssembly.Instance(module,{env});
const words=new Uint32Array(env.memory.buffer);
words.set([base, 0x402100, 0x404000], 0xa00000/4);
words.set([0x402000, 0x402020], 0x403000/4);
words.set([0x404040, 0x404060], 0x404080/4);
for(let position=0;position<order.length;position++) {
 env.eax.value=0;env.esp.value=0x10000;env.resume_next.value=order[position];
 words[0x10000/4]=0x12345678;
 exports.runAt(base);
 assert.equal(env.eax.value,order.length-position, `resume segment ${order[position]}`);
 assert.equal(env.resume_next.value,0);
 assert.equal(env.esp.value,0x10004);
 assert.equal(env.actual_return.value,0x12345678);
}
for (const [index, expected] of [[0,111], [1,222]]) {
 env.eax.value=index;env.edx.value=0;env.esp.value=0x10000;env.resume_next.value=0;
 exports.runAt(0x402100);
 assert.equal(env.edx.value,expected);assert.equal(env.esp.value,0x10004);
}
for (const [index, expected] of [[0,11], [1,22], [-2,11], [-1,22]]) {
 env.ecx.value=index;env.edx.value=0;env.esp.value=0x10000;env.resume_next.value=0;
 exports.runAt(0x404000);
 assert.equal(env.edx.value,expected, `switch index ${index}`);
 assert.equal(env.esp.value,0x10004);
}
for(const invalid of [-1, order.length, 0x7fffffff]) {
 env.eax.value=0;env.esp.value=0x10000;env.resume_next.value=invalid;
 exports.runAt(base);
 assert.equal(env.eax.value,0);assert.equal(env.esp.value,0x10000);
}
"#,
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(&serde_json::to_vec(&input).unwrap())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
