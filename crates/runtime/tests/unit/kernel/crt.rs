use super::*;
use crate::{
    kernel::{resolve, Kernel},
    SparseMemory,
};

fn call(kernel: &mut Kernel, process: &mut Process<SparseMemory>, name: &str, args: &[u32]) -> u32 {
    let api = resolve("MSVCRT.DLL", name).unwrap();
    assert_eq!(
        api.stack_args, 0,
        "cdecl leaves arguments for caller cleanup"
    );
    kernel.call(process, api.id, args).unwrap()
}
fn setup() -> (Kernel, Process<SparseMemory>) {
    (
        Kernel::default(),
        Process::new(SparseMemory::default(), 0x10000, 0x1000000).unwrap(),
    )
}

#[test]
fn allocations_preserve_bytes_and_calloc_checks_overflow() {
    let (mut kernel, mut process) = setup();
    let pointer = call(&mut kernel, &mut process, "malloc", &[8]);
    process.write(pointer, b"abc\0").unwrap();
    let replacement = call(&mut kernel, &mut process, "realloc", &[pointer, 32]);
    let mut data = [0; 4];
    process.read(replacement, &mut data).unwrap();
    assert_eq!(&data, b"abc\0");
    call(&mut kernel, &mut process, "free", &[replacement]);
    let cleared = call(&mut kernel, &mut process, "calloc", &[2, 4]);
    process.read(cleared, &mut data).unwrap();
    assert_eq!(data, [0; 4]);
    assert_eq!(call(&mut kernel, &mut process, "calloc", &[u32::MAX, 2]), 0);
    assert_eq!(call(&mut kernel, &mut process, "realloc", &[cleared, 0]), 0);
}

#[test]
fn memmove_handles_overlap_across_scratch_chunks() {
    let (mut kernel, mut process) = setup();
    let pointer = call(&mut kernel, &mut process, "malloc", &[10000]);
    let original: Vec<u8> = (0..10000).map(|value| (value % 251) as u8).collect();
    process.write(pointer, &original).unwrap();
    assert_eq!(
        call(
            &mut kernel,
            &mut process,
            "memmove",
            &[pointer + 17, pointer, 8000]
        ),
        pointer + 17
    );
    let mut moved = vec![0; 8000];
    process.read(pointer + 17, &mut moved).unwrap();
    assert_eq!(moved, original[..8000]);
    call(
        &mut kernel,
        &mut process,
        "memmove",
        &[pointer, pointer + 17, 8000],
    );
    process.read(pointer, &mut moved).unwrap();
    assert_eq!(moved, original[..8000]);
    assert_eq!(
        call(
            &mut kernel,
            &mut process,
            "memcmp",
            &[pointer, pointer, 8000]
        ),
        0
    );
    call(&mut kernel, &mut process, "memset", &[pointer, 0xff, 32]);
    let mut filled = [0; 32];
    process.read(pointer, &mut filled).unwrap();
    assert_eq!(filled, [0xff; 32]);
}

#[test]
fn byte_strings_respect_counts_padding_unsigned_order_and_nul_search() {
    let (mut kernel, mut process) = setup();
    let source = call(&mut kernel, &mut process, "malloc", &[16]);
    let destination = call(&mut kernel, &mut process, "malloc", &[16]);
    process.write(source, b"AbA\0").unwrap();
    assert_eq!(call(&mut kernel, &mut process, "strlen", &[source]), 3);
    assert_eq!(
        call(&mut kernel, &mut process, "strchr", &[source, b'A' as u32]),
        source
    );
    assert_eq!(
        call(&mut kernel, &mut process, "strrchr", &[source, b'A' as u32]),
        source + 2
    );
    assert_eq!(
        call(&mut kernel, &mut process, "strchr", &[source, 0]),
        source + 3
    );
    call(
        &mut kernel,
        &mut process,
        "strncpy",
        &[destination, source, 8],
    );
    let mut padded = [0xff; 8];
    process.read(destination, &mut padded).unwrap();
    assert_eq!(&padded, b"AbA\0\0\0\0\0");
    process.write(destination, b"aba\0").unwrap();
    assert_eq!(
        call(
            &mut kernel,
            &mut process,
            "_stricmp",
            &[source, destination]
        ),
        0
    );
    assert!((call(&mut kernel, &mut process, "strcmp", &[source, destination]) as i32) < 0);
    assert_eq!(call(&mut kernel, &mut process, "strncmp", &[0, 0, 0]), 0);
    process.write(source, &[0xff, 0]).unwrap();
    process.write(destination, &[0x7f, 0]).unwrap();
    assert!(call(&mut kernel, &mut process, "strcmp", &[source, destination]) as i32 > 0);
    process.write(destination, &[0x55; 8]).unwrap();
    call(
        &mut kernel,
        &mut process,
        "strncpy",
        &[destination, source, 1],
    );
    process.read(destination, &mut padded).unwrap();
    assert_eq!(&padded[..2], &[0xff, 0x55]);
}
