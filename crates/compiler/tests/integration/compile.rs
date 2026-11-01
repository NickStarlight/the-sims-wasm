use sims_compiler::Compiler;

/// One executable section containing RET; no copyrighted guest assets required.
fn return_executable() -> Vec<u8> {
    let mut bytes = vec![0; 0x400];
    fn word(bytes: &mut [u8], at: usize, value: u16) {
        bytes[at..at + 2].copy_from_slice(&value.to_le_bytes());
    }
    fn dword(bytes: &mut [u8], at: usize, value: u32) {
        bytes[at..at + 4].copy_from_slice(&value.to_le_bytes());
    }
    word(&mut bytes, 0, 0x5a4d);
    dword(&mut bytes, 0x3c, 0x80);
    dword(&mut bytes, 0x80, 0x4550);
    word(&mut bytes, 0x84, 0x14c);
    word(&mut bytes, 0x86, 1);
    word(&mut bytes, 0x94, 0xe0);
    word(&mut bytes, 0x98, 0x10b);
    dword(&mut bytes, 0xa8, 0x1000);
    dword(&mut bytes, 0xb4, 0x400000);
    dword(&mut bytes, 0xd0, 0x2000);
    dword(&mut bytes, 0xd4, 0x200);
    let section = 0x178;
    bytes[section..section + 5].copy_from_slice(b".text");
    dword(&mut bytes, section + 8, 0x200);
    dword(&mut bytes, section + 12, 0x1000);
    dword(&mut bytes, section + 16, 0x200);
    dword(&mut bytes, section + 20, 0x200);
    dword(&mut bytes, section + 36, 0x60000020);
    bytes[0x200] = 0xc3;
    bytes
}

#[test]
fn compiles_image_and_relocated_module_from_memory() {
    let executable = return_executable();
    let mut compiler = Compiler::new(&executable).unwrap();
    compiler.add_module(&executable, 0x10000000).unwrap();
    let artifact = compiler.compile(&[0x10001000]).unwrap();
    assert_eq!(artifact.metadata.entry, 0x401000);
    assert_eq!(artifact.metadata.func_addrs, [0x401000, 0x10001000]);
    assert_eq!(artifact.metadata.wasm_bytes, artifact.wasm.len());
    assert!(artifact.metadata.diagnostics.is_empty());
    let metadata: serde_json::Value =
        serde_json::from_str(&artifact.metadata_json().unwrap()).unwrap();
    assert_eq!(metadata["funcCount"], 2);
    assert_eq!(metadata["entry"], 0x401000);
    assert_eq!(metadata["commandLineAddr"], 0xd00000);
    wasmparser::Validator::new()
        .validate_all(&artifact.wasm)
        .unwrap();
}

#[test]
fn images_cannot_overlap_host_memory() {
    for base in [0, 0x800000, 0xa00000, 0xd00000, 0xe00000, 0x1f000000] {
        let mut bytes = return_executable();
        bytes[0xb4..0xb8].copy_from_slice(&u32::to_le_bytes(base));
        assert!(Compiler::new(&bytes).is_err(), "image base {base:#x}");
        let mut compiler = Compiler::new(&return_executable()).unwrap();
        assert!(
            compiler.add_module(&bytes, base).is_err(),
            "DLL base {base:#x}"
        );
    }
}

#[test]
fn runtime_metadata_preserves_linking_fields_without_debug_metadata_or_wasm_changes() {
    let executable = return_executable();
    let mut compiler = Compiler::new(&executable).unwrap();
    compiler.add_module(&executable, 0x10000000).unwrap();
    let mut artifact = compiler.compile(&[0x10001000]).unwrap();
    let wasm = artifact.wasm.clone();
    let full: serde_json::Value = serde_json::from_str(&artifact.metadata_json().unwrap()).unwrap();
    let expected = serde_json::json!({
        "funcAddrs": full["funcAddrs"],
        "funcCount": full["funcCount"],
        "tableAddr": full["tableAddr"],
        "trampBase": full["trampBase"],
        "entry": full["entry"],
        "commandLineAddr": full["commandLineAddr"]
    });
    let lean = artifact.runtime_metadata_json().unwrap();
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&lean).unwrap(),
        expected
    );
    assert!(full["instructionMaps"].as_array().unwrap().len() > 1);

    // Debug metadata can grow independently without leaking into the runtime payload.
    let maps = artifact.metadata.instruction_maps.clone();
    artifact.metadata.instruction_maps.extend(maps);
    artifact.metadata.diagnostics.push("diagnostic only".into());
    artifact.metadata.roots.push(0x12345678);
    artifact.metadata.func_body_starts.push(usize::MAX);
    assert_eq!(artifact.runtime_metadata_json().unwrap(), lean);
    assert_eq!(artifact.wasm, wasm);
    assert_eq!(compiler.compile(&[0x10001000]).unwrap().wasm, wasm);
}

#[test]
fn compiler_preserves_data_at_the_old_command_line_address() {
    let mut bytes = return_executable();
    bytes.resize(0x600, 0);
    bytes[0x86..0x88].copy_from_slice(&2u16.to_le_bytes());
    bytes[0xd0..0xd4].copy_from_slice(&0x301000u32.to_le_bytes());
    let section = 0x178 + 40;
    bytes[section..section + 5].copy_from_slice(b".data");
    for (offset, value) in [
        (8, 0x200u32),
        (12, 0x300000),
        (16, 0x200),
        (20, 0x400),
        (36, 0xc0000040),
    ] {
        bytes[section + offset..section + offset + 4].copy_from_slice(&value.to_le_bytes());
    }
    bytes[0x400..].fill(0xa5);
    let compiler = Compiler::new(&bytes).unwrap();
    let plan = compiler.load_plan();
    let span = plan
        .spans
        .iter()
        .find(|span| span.address == 0x700000)
        .unwrap();
    assert_eq!(
        &plan.bytes[span.offset..span.offset + span.length as usize],
        &bytes[0x400..]
    );
}

#[test]
fn raw_section_padding_can_fill_the_last_image_page() {
    let mut bytes = return_executable();
    bytes[0xd0..0xd4].copy_from_slice(&0x1101u32.to_le_bytes());
    bytes[0x180..0x184].copy_from_slice(&0x101u32.to_le_bytes());
    let compiler = Compiler::new(&bytes).unwrap();
    let plan = compiler.load_plan();
    assert_eq!(plan.images[0].size_of_image, 0x2000);
    assert_eq!(plan.spans[1].length, 0x200);
    compiler.compile(&[]).unwrap();
    // Raw padding is allowed, but a virtual section outside the declared image is not.
    bytes[0x180..0x184].copy_from_slice(&0x102u32.to_le_bytes());
    assert!(Compiler::new(&bytes).is_err());
}

#[test]
fn data_values_inside_known_instructions_are_not_callback_roots() {
    let mut bytes = return_executable();
    bytes.resize(0x600, 0);
    bytes[0x86..0x88].copy_from_slice(&2u16.to_le_bytes());
    bytes[0xd0..0xd4].copy_from_slice(&0x3000u32.to_le_bytes());
    let section = 0x178 + 40;
    bytes[section..section + 6].copy_from_slice(b".rdata");
    for (offset, value) in [
        (8, 0x200u32),
        (12, 0x2000),
        (16, 0x200),
        (20, 0x400),
        (36, 0x40000040),
    ] {
        bytes[section + offset..section + offset + 4].copy_from_slice(&value.to_le_bytes());
    }
    // The entry calls a real function whose MOV immediate looks like FISTTP/RET
    // if a numeric lookup-table value is mistaken for a pointer into its middle.
    bytes[0x200..0x206].copy_from_slice(&[0xe8, 0x0b, 0, 0, 0, 0xc3]);
    bytes[0x210..0x216].copy_from_slice(&[0xb8, 0xdf, 0x08, 0xc3, 0, 0xc3]);
    bytes[0x220] = 0xc3;
    // This speculative MOV starts before the legitimate callback. It must not
    // establish authoritative boundaries which hide that callback's RET.
    bytes[0x21f] = 0xb8;
    bytes[0x224] = 0xc3;
    bytes[0x400..0x404].copy_from_slice(&0x401011u32.to_le_bytes());
    bytes[0x404..0x408].copy_from_slice(&0x401020u32.to_le_bytes());
    bytes[0x408..0x40c].copy_from_slice(&0x40101fu32.to_le_bytes());
    let compiler = Compiler::new(&bytes).unwrap();
    let artifact = compiler.compile(&[]).unwrap();
    assert_eq!(
        artifact.metadata.func_addrs,
        [0x401000, 0x401010, 0x40101f, 0x401020]
    );
    assert!(artifact
        .metadata
        .diagnostics
        .iter()
        .any(|line| line.contains("0x401011")
            && line.contains("conflicts with instruction 0x401010")));
    // Explicit roots still surface genuine unsupported encodings, rather than
    // using speculative filtering to hide them.
    let error = compiler.compile(&[0x401011]).err().unwrap().to_string();
    assert!(error.contains("function 0x401011"), "{error}");
    assert!(error.contains("FPU DF /1 unsupported"), "{error}");
}

#[test]
fn callback_pointers_cover_every_executable_section_in_the_main_image() {
    let mut bytes = return_executable();
    bytes.resize(0x800, 0);
    bytes[0x86..0x88].copy_from_slice(&3u16.to_le_bytes());
    bytes[0xd0..0xd4].copy_from_slice(&0x4000u32.to_le_bytes());
    // Neither executable section needs the conventional .text name.
    bytes[0x178..0x180].copy_from_slice(b"MAINCODE");
    for (section, name, rva, file, flags) in [
        (0x1a0, b"PACODE\0\0", 0x2000u32, 0x400u32, 0x60000020u32),
        (0x1c8, b".data\0\0\0", 0x3000, 0x600, 0xc0000040),
    ] {
        bytes[section..section + 8].copy_from_slice(name);
        for (offset, value) in [(8, 0x200), (12, rva), (16, 0x200), (20, file), (36, flags)] {
            bytes[section + offset..section + offset + 4].copy_from_slice(&value.to_le_bytes());
        }
    }
    // A callback table points into the second executable section, as Complete's
    // vertex-transform dispatch tables point into its separately linked PACODE.
    bytes[0x400] = 0xc3;
    bytes[0x600..0x604].copy_from_slice(&0x402000u32.to_le_bytes());
    let artifact = Compiler::new(&bytes).unwrap().compile(&[]).unwrap();
    assert_eq!(artifact.metadata.func_addrs, [0x401000, 0x402000]);
}

#[test]
fn indirect_calls_read_stack_operands_before_pushing_the_return_address() {
    use std::{
        io::Write,
        process::{Command, Stdio},
    };
    let mut bytes = return_executable();
    // PUSH callback; CALL [ESP]; ADD ESP,4; RET
    bytes[0x200..0x20c].copy_from_slice(&[
        0x68, 0x30, 0x10, 0x40, 0, 0xff, 0x14, 0x24, 0x83, 0xc4, 4, 0xc3,
    ]);
    // Save caller ESP, set ESP=callback, CALL ESP, restore caller ESP, RET.
    bytes[0x210..0x21c].copy_from_slice(&[
        0x8b, 0xf4, 0xbc, 0x30, 0x10, 0x40, 0, 0xff, 0xd4, 0x8b, 0xe6, 0xc3,
    ]);
    bytes[0x230..0x236].copy_from_slice(&[0xb8, 0x42, 0, 0, 0, 0xc3]);
    let artifact = Compiler::new(&bytes).unwrap().compile(&[0x401010]).unwrap();
    let payload = serde_json::json!({ "wasm": artifact.wasm, "metadata": artifact.metadata });
    let mut child = Command::new("node").args(["--input-type=module", "-e", r#"
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
const {wasm,metadata} = JSON.parse(readFileSync(0,'utf8'));
const env = {
  memory: new WebAssembly.Memory({initial:256,maximum:65536}),
  table: new WebAssembly.Table({element:'anyfunc',initial:metadata.trampBase+16384}),
  dispatch() { assert.fail('unexpected guest target'); }, blockTrace() {},
};
for (const name of ['eax','ecx','edx','ebx','esp','ebp','esi','edi','flags','fp_top','fp_status','trap_va','call_target','resume_next','actual_return','teb_base'])
  env[name]=new WebAssembly.Global({value:'i32',mutable:true},0);
const {exports}=new WebAssembly.Instance(new WebAssembly.Module(Uint8Array.from(wasm)),{env});
new Uint32Array(env.memory.buffer).set(metadata.funcAddrs,metadata.tableAddr/4);
for (const entry of [0x401000,0x401010]) {
  env.eax.value=0;env.esp.value=0xe00000;
  exports.run(entry);
  assert.equal(env.eax.value,0x42);
  assert.equal(env.esp.value,0xdffff0);
}
"#]).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(&serde_json::to_vec(&payload).unwrap())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn truncated_images_return_errors() {
    let executable = return_executable();
    for end in [0, 1, 63, 0x83, 0x97, 0x17f, 0x200, 0x3ff] {
        assert!(Compiler::new(&executable[..end]).is_err(), "length {end}");
    }
}

#[test]
fn overflowing_image_fields_return_errors() {
    for (offset, value) in [
        (0x3c, u32::MAX),    // NT header offset
        (0xb4, 0xffff_0000), // preferred load address
        (0xa8, u32::MAX),    // entry RVA
        (0xd4, u32::MAX),    // header extent
        (0x180, u32::MAX),   // section virtual size
        (0x184, u32::MAX),   // section RVA
        (0x188, u32::MAX),   // raw data size
        (0x18c, u32::MAX),   // raw data offset
    ] {
        let mut bytes = return_executable();
        bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
        assert!(Compiler::new(&bytes).is_err(), "field at {offset:#x}");
    }
}

fn relocated_image(page: u32, relocation: u16) -> Vec<u8> {
    let mut bytes = return_executable();
    for (offset, value) in [
        (0xf4, 16u32),
        (0x120, 0x1100),
        (0x124, 12),
        (0x300, page),
        (0x304, 12),
    ] {
        bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    }
    bytes[0x308..0x30a].copy_from_slice(&relocation.to_le_bytes());
    bytes
}

#[test]
fn rejected_relocations_leave_existing_image_unchanged() {
    let executable = return_executable();
    let mut compiler = Compiler::new(&executable).unwrap();
    compiler.add_module(&executable, 0x500000).unwrap();
    let before = compiler.compile(&[0x501000]).unwrap();
    for (page, relocation) in [(0x1000, 0x5010), (u32::MAX, 0x3010), (0x2000, 0x3010)] {
        let mut malformed = relocated_image(page, relocation);
        malformed[0x200] = 0xcc; // Detect accidental writes before the rejected relocation.
        assert!(compiler.add_module(&malformed, 0x500000).is_err());
        let after = compiler.compile(&[0x501000]).unwrap();
        assert_eq!(before.wasm, after.wasm);
        assert_eq!(before.metadata.func_addrs, after.metadata.func_addrs);
    }
}

fn executable_with_imports() -> Vec<u8> {
    let mut bytes = return_executable();
    for (offset, value) in [
        (0xf4, 16u32),
        (0x100, 0x1020),
        (0x104, 40),
        (0x220, 0x10a0),
        (0x22c, 0x1060),
        (0x230, 0x10b0),
        (0x2a0, 0x1080),
        (0x2a4, 0x8000002a),
        (0x2b0, 0x1080),
        (0x2b4, 0x8000002a),
    ] {
        bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    }
    bytes[0x260..0x26d].copy_from_slice(b"KERNEL32.dll\0");
    bytes[0x280..0x282].copy_from_slice(&7u16.to_le_bytes());
    bytes[0x282..0x28c].copy_from_slice(b"HeapAlloc\0");
    bytes
}

#[test]
fn preserves_named_and_ordinal_imports() {
    let mut bytes = executable_with_imports();
    for original in [0x10a0u32, 0] {
        bytes[0x220..0x224].copy_from_slice(&original.to_le_bytes());
        let artifact = Compiler::new(&bytes).unwrap().compile(&[]).unwrap();
        let imports = &artifact.metadata.imports;
        assert_eq!(imports.len(), 2);
        assert_eq!(imports[0].dll, "KERNEL32.dll");
        assert_eq!(imports[0].name.as_deref(), Some("HeapAlloc"));
        assert_eq!(imports[0].hint, Some(7));
        assert_eq!(imports[0].iat_va, 0x4010b0);
        assert_eq!(imports[1].ordinal, Some(42));
        assert_eq!(imports[1].iat_va, 0x4010b4);
    }
    bytes[0x230..0x234].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(Compiler::new(&bytes).is_err());
}

#[test]
fn accepts_unaligned_input_slices() {
    let mut bytes = vec![0];
    bytes.extend(return_executable());
    assert!(Compiler::new(&bytes[1..]).is_ok());
}

#[test]
fn overlapping_module_loads_leave_images_unchanged() {
    let executable = return_executable();
    let mut compiler = Compiler::new(&executable).unwrap();
    compiler.add_module(&executable, 0x500000).unwrap();
    let before = compiler.compile(&[0x501000]).unwrap();
    let mut replacement = executable.clone();
    replacement[0x200] = 0xcc;
    for base in [0x400000, 0x401000, 0x3ff000, 0x500000, 0x501000, 0x4ff000] {
        let error = compiler.add_module(&replacement, base).unwrap_err();
        assert!(error.to_string().contains("overlaps an existing image"));
    }
    let after = compiler.compile(&[0x501000]).unwrap();
    assert_eq!(before.wasm, after.wasm);
    assert_eq!(
        before.metadata_json().unwrap(),
        after.metadata_json().unwrap()
    );
    // Adjacent ranges are valid, proving the check uses half-open image ranges.
    compiler.add_module(&executable, 0x502000).unwrap();
}

#[test]
fn stripped_relocations_require_the_preferred_base() {
    let mut stripped = return_executable();
    stripped[0x96..0x98].copy_from_slice(&1u16.to_le_bytes());
    let mut compiler = Compiler::new(&return_executable()).unwrap();
    let error = compiler.add_module(&stripped, 0x500000).unwrap_err();
    assert!(error.to_string().contains("stripped relocations"));
    // A failed rebase must not occupy the requested address range.
    let preferred_base = 0x500000u32;
    stripped[0xb4..0xb8].copy_from_slice(&preferred_base.to_le_bytes());
    compiler.add_module(&stripped, preferred_base).unwrap();
    assert_eq!(compiler.compile(&[]).unwrap().metadata.modules.len(), 1);
}

#[test]
fn module_metadata_exposes_relocated_import_slots() {
    let executable = executable_with_imports();
    let mut compiler = Compiler::new(&executable).unwrap();
    compiler.add_module(&executable, 0x500000).unwrap();
    let artifact = compiler.compile(&[]).unwrap();
    // Existing EXE imports retain their original addresses.
    assert_eq!(artifact.metadata.imports[0].iat_va, 0x4010b0);
    let module = &artifact.metadata.modules[0];
    assert_eq!(module.base, 0x500000);
    assert_eq!(module.preferred_base, 0x400000);
    assert_eq!(module.size_of_image, 0x2000);
    assert_eq!(module.imports[0].iat_va, 0x5010b0);
    assert_eq!(module.imports[0].name.as_deref(), Some("HeapAlloc"));
    assert_eq!(module.imports[1].iat_va, 0x5010b4);
    assert_eq!(module.imports[1].ordinal, Some(42));
    let json: serde_json::Value = serde_json::from_str(&artifact.metadata_json().unwrap()).unwrap();
    assert_eq!(json["modules"][0]["base"], 0x500000);
    assert_eq!(json["modules"][0]["preferredBase"], 0x400000);
    assert_eq!(json["modules"][0]["imports"][0]["iatVa"], 0x5010b0);
}

#[test]
fn zero_filled_entry_returns_an_actionable_error() {
    let mut bytes = return_executable();
    bytes[0x200] = 0;
    let error = Compiler::new(&bytes).unwrap().compile(&[]).err().unwrap();
    assert!(error
        .to_string()
        .contains("cannot compile entry point 0x401000"));
    assert!(error
        .to_string()
        .contains("outside mapped executable sections"));
}

fn executable_with_exports(target: u32) -> Vec<u8> {
    let mut bytes = return_executable();
    for (offset, value) in [
        (0xf4, 16u32),
        (0xf8, 0x1100),
        (0xfc, 0xa0),
        (0x30c, 0x1180),
        (0x310, 7),
        (0x314, 1),
        (0x318, 1),
        (0x31c, 0x1140),
        (0x320, 0x1150),
        (0x324, 0x1160),
        (0x340, target),
        (0x350, 0x1170),
    ] {
        bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    }
    bytes[0x96..0x98].copy_from_slice(&0x2000u16.to_le_bytes());
    bytes[0x370..0x379].copy_from_slice(b"Function\0");
    bytes[0x380..0x38a].copy_from_slice(b"guest.dll\0");
    bytes
}

#[test]
fn load_plan_exports_use_mapped_addresses_and_explicit_forwarders() {
    use sims_compiler::ExportTarget;
    let mut compiler = Compiler::new(&return_executable()).unwrap();
    compiler
        .add_module(&executable_with_exports(0x1000), 0x500000)
        .unwrap();
    let plan = compiler.load_plan();
    let dll = &plan.images[1];
    assert_eq!(dll.name.as_deref(), Some("guest.dll"));
    assert!(dll.is_dll);
    assert_eq!(dll.entry, Some(0x501000));
    assert_eq!(dll.exports[0].ordinal, 7);
    assert_eq!(dll.exports[0].name.as_deref(), Some("Function"));
    assert!(matches!(
        dll.exports[0].target,
        ExportTarget::Address { address: 0x501000 }
    ));
    for (forward, ordinal) in [
        (b"other.Name\0".as_slice(), false),
        (b"other.#42\0".as_slice(), true),
    ] {
        let mut bytes = executable_with_exports(0x1190);
        bytes[0x390..0x390 + forward.len()].copy_from_slice(forward);
        let mut compiler = Compiler::new(&return_executable()).unwrap();
        compiler.add_module(&bytes, 0x500000).unwrap();
        let plan = compiler.load_plan();
        match &plan.images[1].exports[0].target {
            ExportTarget::ForwardByOrdinal {
                dll,
                ordinal: value,
            } => {
                assert!(ordinal);
                assert_eq!(dll, "other");
                assert_eq!(*value, 42);
            }
            ExportTarget::ForwardByName { dll, name } => {
                assert!(!ordinal);
                assert_eq!(dll, "other");
                assert_eq!(name, "Name");
            }
            _ => panic!("forwarder emitted as guest address"),
        }
    }
}

#[test]
fn load_plan_copies_headers_relocations_and_zero_fills_image_gaps() {
    let mut bytes = relocated_image(0x1000, 0x3010);
    bytes[0x210..0x214].copy_from_slice(&0x401000u32.to_le_bytes());
    let mut compiler = Compiler::new(&return_executable()).unwrap();
    compiler.add_module(&bytes, 0x500000).unwrap();
    let plan = compiler.load_plan();
    let mut mapped = vec![0xaa; 0x2000];
    let dll = &plan.images[1];
    mapped[..dll.size_of_image as usize].fill(0);
    for span in &plan.spans {
        if span.address < dll.base || span.address >= dll.base + dll.size_of_image {
            continue;
        }
        let destination = (span.address - dll.base) as usize;
        let length = span.length as usize;
        mapped[destination..destination + length]
            .copy_from_slice(&plan.bytes[span.offset..span.offset + length]);
    }
    assert_eq!(&mapped[..2], b"MZ");
    assert_eq!(mapped[0x1000], 0xc3);
    assert_eq!(&mapped[0x1010..0x1014], &0x501000u32.to_le_bytes());
    assert!(mapped[0x200..0x1000].iter().all(|byte| *byte == 0));
    assert!(mapped[0x1200..].iter().all(|byte| *byte == 0));
    let json: serde_json::Value = serde_json::from_str(&plan.metadata_json().unwrap()).unwrap();
    assert!(json.get("bytes").is_none());
    assert_eq!(json["images"][1]["base"], 0x500000);
}
