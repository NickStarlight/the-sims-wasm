use super::*;
use crate::SparseMemory;

fn node(key: &str, value: &[u8], text: bool, children: &[Vec<u8>]) -> Vec<u8> {
    let mut bytes = vec![0; 6];
    bytes[2..4].copy_from_slice(&((value.len() / if text { 2 } else { 1 }) as u16).to_le_bytes());
    bytes[4..6].copy_from_slice(&u16::from(text).to_le_bytes());
    for unit in key.encode_utf16().chain([0]) {
        bytes.extend(unit.to_le_bytes());
    }
    bytes.resize((bytes.len() + 3) & !3, 0);
    bytes.extend(value);
    for child in children {
        bytes.resize((bytes.len() + 3) & !3, 0);
        bytes.extend(child);
    }
    let length = bytes.len() as u16;
    bytes[..2].copy_from_slice(&length.to_le_bytes());
    bytes
}
fn raw_version() -> Vec<u8> {
    let mut fixed = [0; 52];
    fixed[..4].copy_from_slice(&0xfeef04bdu32.to_le_bytes());
    let company: Vec<u8> = "Maxis é\0"
        .encode_utf16()
        .flat_map(u16::to_le_bytes)
        .collect();
    let strings = node(
        "StringFileInfo",
        &[],
        true,
        &[node(
            "040904e4",
            &[],
            true,
            &[node("CompanyName", &company, true, &[])],
        )],
    );
    let vars = node(
        "VarFileInfo",
        &[],
        true,
        &[node("Translation", &[9, 4, 0xe4, 4], false, &[])],
    );
    node("VS_VERSION_INFO", &fixed, false, &[strings, vars])
}
fn executable(raw: &[u8]) -> Vec<u8> {
    let mut bytes = vec![0; 0x800];
    for (offset, value) in [
        (0, 0x5a4du16),
        (0x84, 0x14c),
        (0x86, 1),
        (0x94, 0xe0),
        (0x98, 0x10b),
    ] {
        bytes[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
    }
    for (offset, value) in [
        (0x3c, 0x80u32),
        (0x80, 0x4550),
        (0xb4, 0x400000),
        (0xd0, 0x2000),
        (0xd4, 0x200),
        (0xf4, 16),
        (0x108, 0x1000),
        (0x10c, 0x600),
        (0x180, 0x600),
        (0x184, 0x1000),
        (0x188, 0x600),
        (0x18c, 0x200),
    ] {
        bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    }
    bytes[0x178..0x17d].copy_from_slice(b".rsrc");
    for (table, id, target) in [
        (0, 16u32, 0x80000018u32),
        (0x18, 1, 0x80000030),
        (0x30, 0x409, 0x48),
    ] {
        let offset = 0x200 + table;
        bytes[offset + 14..offset + 16].copy_from_slice(&1u16.to_le_bytes());
        bytes[offset + 16..offset + 20].copy_from_slice(&id.to_le_bytes());
        bytes[offset + 20..offset + 24].copy_from_slice(&target.to_le_bytes());
    }
    bytes[0x248..0x24c].copy_from_slice(&0x1100u32.to_le_bytes());
    bytes[0x24c..0x250].copy_from_slice(&(raw.len() as u32).to_le_bytes());
    bytes[0x300..0x300 + raw.len()].copy_from_slice(raw);
    bytes
}
fn put(process: &mut Process<SparseMemory>, bytes: &[u8]) -> u32 {
    let pointer = process.heap_alloc(bytes.len() as u32).unwrap();
    process.write(pointer, bytes).unwrap();
    pointer
}
fn call(kernel: &mut Kernel, process: &mut Process<SparseMemory>, name: &str, args: &[u32]) -> u32 {
    kernel
        .call(process, resolve("version.dll", name).unwrap().id, args)
        .unwrap()
}
fn read_u32(process: &Process<SparseMemory>, address: u32) -> u32 {
    let mut bytes = [0; 4];
    process.read(address, &mut bytes).unwrap();
    u32::from_le_bytes(bytes)
}
#[test]
fn version_queries_return_real_fixed_translation_and_ansi_string_data() {
    let raw = raw_version();
    assert_eq!(resource::resource(&executable(&raw)).unwrap(), raw);
    let mut kernel = Kernel::default();
    kernel.add_file("fixture.exe", &executable(&raw)).unwrap();
    let mut process = Process::new(SparseMemory::default(), 0x10000, 0x1000000).unwrap();
    let name = put(&mut process, b"fixture.exe\0");
    let size = call(
        &mut kernel,
        &mut process,
        "GetFileVersionInfoSizeA",
        &[name, 0],
    );
    assert!(size > raw.len() as u32);
    let data = process.heap_alloc(size).unwrap();
    assert_eq!(
        call(
            &mut kernel,
            &mut process,
            "GetFileVersionInfoA",
            &[name, 0, size, data]
        ),
        1
    );
    let pointer = put(&mut process, &[0; 4]);
    let length = put(&mut process, &[0; 4]);
    for (path, expected) in [
        (b"\\\0".as_slice(), raw[40..92].to_vec()),
        (
            b"\\VarFileInfo\\Translation\0".as_slice(),
            vec![9, 4, 0xe4, 4],
        ),
        (
            b"\\StringFileInfo\\040904e4\\CompanyName\0".as_slice(),
            b"Maxis \xe9\0".to_vec(),
        ),
    ] {
        let path = put(&mut process, path);
        assert_eq!(
            call(
                &mut kernel,
                &mut process,
                "VerQueryValueA",
                &[data, path, pointer, length]
            ),
            1
        );
        let count = read_u32(&process, length);
        assert_eq!(count as usize, expected.len());
        let mut result = vec![0; count as usize];
        process
            .read(read_u32(&process, pointer), &mut result)
            .unwrap();
        assert_eq!(result, expected);
    }
}
#[test]
fn missing_and_malformed_resources_report_failure_not_fake_versions() {
    let mut kernel = Kernel::default();
    let mut process = Process::new(SparseMemory::default(), 0x10000, 0x1000000).unwrap();
    let name = put(&mut process, b"absent.exe\0");
    assert_eq!(
        call(
            &mut kernel,
            &mut process,
            "GetFileVersionInfoSizeA",
            &[name, 0]
        ),
        0
    );
    assert_eq!(kernel.thread().error, 2);
    kernel.add_file("absent.exe", b"invalid PE").unwrap();
    assert_eq!(
        call(
            &mut kernel,
            &mut process,
            "GetFileVersionInfoSizeA",
            &[name, 0]
        ),
        0
    );
    assert_eq!(kernel.thread().error, 13);
    let mut raw = raw_version();
    raw[..2].copy_from_slice(&u16::MAX.to_le_bytes());
    assert!(block::unpack(&raw).is_err());
}
