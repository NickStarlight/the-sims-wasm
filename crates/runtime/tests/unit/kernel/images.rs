use super::*;
use crate::SparseMemory;

fn executable() -> Vec<u8> {
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
    fn table(bytes: &mut [u8], offset: usize, entries: &[(u32, u32)]) {
        let offset = 0x200 + offset;
        bytes[offset + 14..offset + 16].copy_from_slice(&(entries.len() as u16).to_le_bytes());
        for (index, (id, target)) in entries.iter().enumerate() {
            let entry = offset + 16 + index * 8;
            bytes[entry..entry + 4].copy_from_slice(&id.to_le_bytes());
            bytes[entry + 4..entry + 8].copy_from_slice(&target.to_le_bytes());
        }
    }
    table(&mut bytes, 0, &[(3, 0x80000020), (14, 0x80000038)]);
    table(&mut bytes, 0x20, &[(1, 0x80000050)]);
    table(&mut bytes, 0x38, &[(127, 0x80000068)]);
    table(&mut bytes, 0x50, &[(0, 0x80)]);
    table(&mut bytes, 0x68, &[(0x409, 0x90)]);
    for (offset, rva, size) in [(0x280, 0x1100u32, 40u32), (0x290, 0x1140, 20)] {
        bytes[offset..offset + 4].copy_from_slice(&rva.to_le_bytes());
        bytes[offset + 4..offset + 8].copy_from_slice(&size.to_le_bytes());
    }
    // A real BITMAPINFOHEADER payload retained by the handle.
    bytes[0x300..0x304].copy_from_slice(&40u32.to_le_bytes());
    bytes[0x340..0x346].copy_from_slice(&[0, 0, 1, 0, 1, 0]);
    bytes[0x346..0x34e].copy_from_slice(&[32, 32, 0, 0, 1, 0, 32, 0]);
    bytes[0x34e..0x352].copy_from_slice(&40u32.to_le_bytes());
    bytes[0x352..0x354].copy_from_slice(&1u16.to_le_bytes());
    bytes
}

#[test]
fn icon_handles_cache_real_group_and_image_data_and_missing_resource_fails() {
    let mut kernel = Kernel::default();
    kernel
        .install_modules(&crate::modules::tests::configuration())
        .unwrap();
    kernel
        .add_file("C:\\Games\\Sims.exe", &executable())
        .unwrap();
    let mut process = Process::new(SparseMemory::default(), 0x1000, 0x100000).unwrap();
    let api = resolve("user32", "LoadIconA").unwrap();
    let handle = kernel.call(&mut process, api.id, &[0x400000, 127]).unwrap();
    assert_ne!(handle, 0);
    assert_eq!(
        kernel.call(&mut process, api.id, &[0x400000, 127]).unwrap(),
        handle
    );
    let (
        _,
        Asset::Icon {
            _group: group,
            _images: images,
        },
    ) = kernel
        .images
        .entries
        .get(&(0x400000, Name::Id(127), false))
        .unwrap()
    else {
        panic!()
    };
    assert_eq!(group.len(), 20);
    assert_eq!(images[0], executable()[0x300..0x328]);
    assert_eq!(
        kernel.call(&mut process, api.id, &[0x400000, 128]).unwrap(),
        0
    );
    assert_eq!(kernel.thread().error, 1814);
    let mut malformed = executable();
    malformed[0x34e] = 41;
    assert!(load_icon(&malformed, &Name::Id(127)).is_err());
}

#[test]
fn standard_cursor_identity_is_owned_and_cached() {
    let mut kernel = Kernel::default();
    let mut process = Process::new(SparseMemory::default(), 0x1000, 0x100000).unwrap();
    let api = resolve("user32", "LoadCursorA").unwrap();
    let arrow = kernel.call(&mut process, api.id, &[0, 32512]).unwrap();
    assert_ne!(arrow, 0);
    assert_eq!(
        kernel.call(&mut process, api.id, &[0, 32512]).unwrap(),
        arrow
    );
    assert_ne!(
        kernel.call(&mut process, api.id, &[0, 32513]).unwrap(),
        arrow
    );
    assert_eq!(kernel.call(&mut process, api.id, &[0, 1]).unwrap(), 0);
    assert!(kernel.call(&mut process, api.id, &[0x400000, 127]).is_err());
}
