use sims_runtime::{Error, GuestMemory, Process, SparseMemory};

fn process() -> Process<SparseMemory> {
    Process::new(SparseMemory::default(), 0x10000, 0x40000).unwrap()
}

#[test]
fn protected_and_loaded_ranges_cannot_be_reallocated_or_overwritten() {
    let mut process = process();
    process.reserve(0x10000, 0x10000).unwrap();
    process.map(0x30000, b"executable", false).unwrap();
    assert_eq!(process.virtual_alloc(0x10000, 16), Err(Error::Overlap));
    assert_eq!(process.virtual_alloc(0x30000, 16), Err(Error::Overlap));
    assert_eq!(process.write(0x10000, &[0]), Err(Error::Protected));
    assert_eq!(process.write(0x30000, &[0]), Err(Error::Protected));
    assert_eq!(process.heap_alloc(1).unwrap(), 0x20000);
    let mut bytes = [0; 10];
    process.read(0x30000, &mut bytes).unwrap();
    assert_eq!(&bytes, b"executable");
}

#[test]
fn dword_updates_keep_read_before_write_error_precedence() {
    use sims_runtime::U32Update;
    let mut process = process();
    process.map(0x2000, &[7; 2], false).unwrap();
    let operation = U32Update::CompareExchange {
        exchange: 9,
        comparand: 0,
    };
    assert_eq!(process.update_u32(0x2000, operation), Err(Error::Unmapped));
    process.map(0x2002, &[8; 2], true).unwrap();
    assert_eq!(process.update_u32(0x2000, operation), Err(Error::Protected));
    let mut bytes = [0; 4];
    process.read(0x2000, &mut bytes).unwrap();
    assert_eq!(bytes, [7, 7, 8, 8]);
}

#[test]
fn guest_writes_cannot_forge_heap_ownership() {
    let mut process = process();
    let allocation = process.heap_alloc(32).unwrap();
    process.write(allocation, &[0xff; 32]).unwrap();
    assert_eq!(
        process.heap_free(allocation + 16),
        Err(Error::InvalidAllocation)
    );
    assert_eq!(process.heap_size(allocation).unwrap(), 32);
    assert_eq!(
        process.virtual_free(allocation),
        Err(Error::InvalidAllocation)
    );
    process.heap_free(allocation).unwrap();
    assert_eq!(process.heap_free(allocation), Err(Error::InvalidAllocation));
    let reused = process.heap_alloc(32).unwrap();
    assert_eq!(reused, allocation);
    let mut bytes = [1; 32];
    process.read(reused, &mut bytes).unwrap();
    assert_eq!(bytes, [0; 32]);
}

#[test]
fn freeing_adjacent_blocks_reuses_a_coalesced_region() {
    let mut process = process();
    let first = process.heap_alloc(32).unwrap();
    let second = process.heap_alloc(32).unwrap();
    process.heap_free(first).unwrap();
    process.heap_free(second).unwrap();
    let combined = process.heap_alloc(64).unwrap();
    assert_eq!(combined, first);
    process.write(combined, &[7; 64]).unwrap();
    let mut bytes = [0; 64];
    process.read(combined, &mut bytes).unwrap();
    assert_eq!(bytes, [7; 64]);
}

#[test]
fn realloc_preserves_data_and_failure_preserves_old_allocation() {
    let mut process = process();
    let original = process.heap_alloc(4).unwrap();
    process.write(original, b"game").unwrap();
    // A live neighbor prevents in-place growth, exercising relocation and old ownership removal.
    process.heap_alloc(16).unwrap();
    let grown = process.heap_realloc(original, 32).unwrap();
    assert_eq!(process.heap_size(original), Err(Error::InvalidAllocation));
    let mut bytes = [0; 32];
    process.read(grown, &mut bytes).unwrap();
    assert_eq!(&bytes[..4], b"game");
    assert_eq!(&bytes[4..], &[0; 28]);
    assert_eq!(
        process.heap_realloc(grown, 0x40000),
        Err(Error::OutOfMemory)
    );
    assert_eq!(process.heap_size(grown).unwrap(), 32);
    process.read(grown, &mut bytes).unwrap();
    assert_eq!(&bytes[..4], b"game");
}

#[test]
fn realloc_shrinks_and_reuses_only_released_capacity() {
    let mut process = process();
    let allocation = process.heap_alloc(96).unwrap();
    let neighbor = process.heap_alloc(16).unwrap();
    process.write(allocation, &[0xa5; 96]).unwrap();
    assert_eq!(process.heap_realloc(allocation, 17).unwrap(), allocation);
    assert_eq!(process.heap_size(allocation).unwrap(), 17);
    let tail = process.heap_alloc(64).unwrap();
    assert_eq!(tail, allocation + 32);
    let mut bytes = [0; 17];
    process.read(allocation, &mut bytes).unwrap();
    assert_eq!(bytes, [0xa5; 17]);
    process.heap_free(tail).unwrap();
    assert_eq!(process.heap_realloc(allocation, 80).unwrap(), allocation);
    let mut bytes = [0xff; 80];
    process.read(allocation, &mut bytes).unwrap();
    assert_eq!(&bytes[..17], &[0xa5; 17]);
    assert_eq!(&bytes[17..], &[0; 63]);
    assert_eq!(process.heap_size(neighbor).unwrap(), 16);
    assert_eq!(process.heap_alloc(16).unwrap(), allocation + 80);
}

#[test]
fn realloc_grows_within_alignment_and_at_free_frontier_without_moving() {
    let mut process = process();
    let allocation = process.heap_alloc(3).unwrap();
    process.write(allocation, &[0xa5; 16]).unwrap();
    assert_eq!(process.heap_realloc(allocation, 12).unwrap(), allocation);
    let mut bytes = [1; 12];
    process.read(allocation, &mut bytes).unwrap();
    assert_eq!(&bytes[..3], &[0xa5; 3]);
    assert_eq!(&bytes[3..], &[0; 9]);
    assert_eq!(process.heap_realloc(allocation, 64).unwrap(), allocation);
    assert_eq!(process.heap_size(allocation).unwrap(), 64);
    process.write(allocation + 63, &[42]).unwrap();
    assert_eq!(process.heap_alloc(16).unwrap(), allocation + 64);
    assert_eq!(process.heap_realloc(allocation, 0).unwrap(), allocation);
    assert_eq!(process.heap_size(allocation).unwrap(), 0);
    assert_eq!(process.heap_alloc(48).unwrap(), allocation + 16);
}

#[test]
fn realloc_cannot_grow_into_reserved_or_image_bytes() {
    for virtual_reservation in [false, true] {
        let mut process = Process::new(SparseMemory::default(), 0x1fff0, 0x50000).unwrap();
        let allocation = process.heap_alloc(16).unwrap();
        process.write(allocation, b"keep").unwrap();
        if virtual_reservation {
            process
                .virtual_alloc_flags(0x20000, 0x10000, 0x2000, 1)
                .unwrap();
        } else {
            process.map(0x20000, b"image", false).unwrap();
        }
        let replacement = process.heap_realloc(allocation, 48).unwrap();
        assert_ne!(replacement, allocation);
        let mut bytes = [0; 4];
        process.read(replacement, &mut bytes).unwrap();
        assert_eq!(&bytes, b"keep");
        assert_eq!(process.heap_size(allocation), Err(Error::InvalidAllocation));
        if virtual_reservation {
            assert_eq!(process.read(0x20000, &mut bytes), Err(Error::Unmapped));
            process.virtual_alloc_flags(0x20000, 16, 0x1000, 4).unwrap();
            process.read(0x20000, &mut bytes).unwrap();
            assert_eq!(bytes, [0; 4]);
        } else {
            process.read(0x20000, &mut bytes).unwrap();
            assert_eq!(&bytes, b"imag");
            assert_eq!(process.write(0x20000, b"x"), Err(Error::Protected));
        }
    }
}

#[test]
fn free_size_index_survives_splits_shrinks_and_coalescing() {
    let mut process = process();
    let large = process.heap_alloc(96).unwrap();
    let separator = process.heap_alloc(16).unwrap();
    let small = process.heap_alloc(32).unwrap();
    process.heap_free(large).unwrap();
    process.heap_free(small).unwrap();
    assert_eq!(process.heap_alloc(17).unwrap(), small);
    assert_eq!(process.heap_alloc(80).unwrap(), large);
    assert_eq!(process.heap_realloc(large, 16).unwrap(), large);
    process.heap_free(separator).unwrap();
    assert_eq!(process.heap_alloc(96).unwrap(), large + 16);
    assert_eq!(process.heap_size(small).unwrap(), 17);
}

#[test]
fn process_lifetimes_do_not_share_metadata_or_guest_bytes() {
    let mut first = process();
    let address = first.heap_alloc(4).unwrap();
    first.write(address, b"data").unwrap();
    let mut second = process();
    assert_eq!(second.heap_free(address), Err(Error::InvalidAllocation));
    assert_eq!(second.heap_alloc(4).unwrap(), address);
    let mut bytes = [1; 4];
    second.read(address, &mut bytes).unwrap();
    assert_eq!(bytes, [0; 4]);
    drop(first);
    assert_eq!(second.heap_size(address).unwrap(), 4);
}

#[test]
fn virtual_pages_require_exact_ownership_and_are_zeroed_on_reuse() {
    let mut process = process();
    let address = process.virtual_alloc(0, 1).unwrap();
    assert_eq!(address % 65536, 0);
    process.write(address, b"test").unwrap();
    assert_eq!(
        process.virtual_free(address + 1),
        Err(Error::InvalidAllocation)
    );
    process.virtual_free(address).unwrap();
    assert_eq!(process.read(address, &mut [0; 4]), Err(Error::Unmapped));
    assert_eq!(process.virtual_alloc(address, 1).unwrap(), address);
    let mut bytes = [1; 4];
    process.read(address, &mut bytes).unwrap();
    assert_eq!(bytes, [0; 4]);
}

#[test]
fn address_overflow_and_gaps_are_rejected() {
    let mut process = process();
    assert_eq!(process.reserve(u32::MAX - 1, 8), Err(Error::InvalidRange));
    assert_eq!(process.virtual_alloc(1, 4), Err(Error::InvalidRange));
    assert_eq!(process.heap_alloc(u32::MAX), Err(Error::OutOfMemory));
    process.map(0x1000, &[1, 2], true).unwrap();
    process.map(0x1004, &[3, 4], true).unwrap();
    assert_eq!(process.write(0x1000, &[9; 6]), Err(Error::Unmapped));
    let mut bytes = [0; 2];
    process.read(0x1000, &mut bytes).unwrap();
    assert_eq!(bytes, [1, 2]);
}

#[test]
fn native_backing_memmove_handles_overlap_across_pages() {
    let mut memory = SparseMemory::default();
    memory.ensure(20000).unwrap();
    let original: Vec<_> = (0..9000).map(|i| (i % 251) as u8).collect();
    memory.write(100, &original).unwrap();
    memory.copy(200, 100, 9000).unwrap();
    let mut actual = vec![0; 9000];
    memory.read(200, &mut actual).unwrap();
    assert_eq!(actual, original);
    memory.copy(50, 200, 9000).unwrap();
    memory.read(50, &mut actual).unwrap();
    assert_eq!(actual, original);
}

#[test]
fn crt_reservation_commits_subranges_without_clobbering_live_pages() {
    let mut process = Process::new(SparseMemory::default(), 0x1000000, 0x4000000).unwrap();
    let base = 0x1010000;
    assert_eq!(
        process
            .virtual_alloc_flags(base, 0x100000, 0x2000, 4)
            .unwrap(),
        base
    );
    assert_eq!(process.read(base, &mut [0]), Err(Error::Unmapped));
    assert_eq!(
        process
            .virtual_alloc_flags(base, 0x8000, 0x1000, 4)
            .unwrap(),
        base
    );
    process.write(base, b"CRT").unwrap();
    // A wider second commit overlaps live pages and commits fresh zero pages.
    process
        .virtual_alloc_flags(base, 0x10000, 0x1000, 4)
        .unwrap();
    let mut bytes = [0; 3];
    process.read(base, &mut bytes).unwrap();
    assert_eq!(&bytes, b"CRT");
    process.read(base + 0x8000, &mut bytes).unwrap();
    assert_eq!(bytes, [0; 3]);
    assert_eq!(process.read(base + 0x10000, &mut [0]), Err(Error::Unmapped));
    assert_eq!(
        process.virtual_alloc_flags(base + 0xff000, 0x2000, 0x1000, 4),
        Err(Error::Overlap)
    );
    assert_eq!(
        process.virtual_alloc_flags(base, 0x1000, 0x2000, 4),
        Err(Error::Overlap)
    );
    process.virtual_free(base).unwrap();
    assert_eq!(process.read(base, &mut [0]), Err(Error::Unmapped));
}

#[test]
fn noaccess_reservations_allow_later_writable_commits() {
    let mut process = Process::new(SparseMemory::default(), 0x1000000, 0x40000000).unwrap();
    let base = process
        .virtual_alloc_flags(0, 0x9600000, 0x2000, 1)
        .unwrap();
    assert_eq!(process.read(base, &mut [0]), Err(Error::Unmapped));
    assert_eq!(process.write(base, &[7]), Err(Error::Unmapped));
    let committed = base + 0x10000;
    assert_eq!(
        process
            .virtual_alloc_flags(committed, 4096, 0x1000, 4)
            .unwrap(),
        committed
    );
    process.write(committed, b"arena").unwrap();
    process
        .virtual_alloc_flags(committed, 8192, 0x1000, 4)
        .unwrap();
    let mut bytes = [0; 5];
    process.read(committed, &mut bytes).unwrap();
    assert_eq!(&bytes, b"arena");
    process.read(committed + 4096, &mut bytes).unwrap();
    assert_eq!(bytes, [0; 5]);
    assert_eq!(
        process.read(committed + 8192, &mut [0]),
        Err(Error::Unmapped)
    );
    process.virtual_decommit(committed, 4096).unwrap();
    assert_eq!(process.read(committed, &mut [0]), Err(Error::Unmapped));
    process.virtual_free(base).unwrap();
    assert_eq!(
        process.read(committed + 4096, &mut [0]),
        Err(Error::Unmapped)
    );
}

#[test]
fn commits_cannot_claim_image_heap_or_protected_ranges() {
    let mut process = process();
    process.map(0x1000, &[0; 4096], true).unwrap();
    process.reserve(0x2000, 4096).unwrap();
    let heap = process.heap_alloc(4096).unwrap();
    for address in [0x1000, 0x2000, heap] {
        assert_eq!(
            process.virtual_alloc_flags(address, 1, 0x1000, 4),
            Err(Error::Overlap)
        );
    }
}

#[test]
fn virtual_ranges_round_addresses_and_page_spanning_sizes() {
    let mut process = process();
    assert_eq!(
        process
            .virtual_alloc_flags(0x10001, 0x3000, 0x2000, 4)
            .unwrap(),
        0x10000
    );
    assert_eq!(
        process.virtual_alloc_flags(0x10fff, 2, 0x1000, 4).unwrap(),
        0x10000
    );
    process.write(0x10000, &[1]).unwrap();
    process.write(0x11fff, &[1]).unwrap();
    assert_eq!(process.write(0x12000, &[1]), Err(Error::Unmapped));
    process.virtual_free(0x10000).unwrap();
    // Uncommitted reservations can also be released without growing backing memory.
    process
        .virtual_alloc_flags(0x30000, 4096, 0x2000, 4)
        .unwrap();
    process.virtual_free(0x30000).unwrap();
}

#[test]
fn bounded_guest_strings_stop_at_nul_without_reading_through_gaps() {
    let mut process = process();
    process.map(0x1000, b"name\0", false).unwrap();
    assert_eq!(process.read_c_string(0x1000, 128).unwrap(), b"name");
    assert!(process.read_c_string(0x1000, 4).is_err());
    assert_eq!(process.read_c_string(0x1005, 128), Err(Error::Unmapped));
}

#[test]
fn decommit_retains_reservation_and_recommit_zeroes_only_released_pages() {
    let mut p = Process::new(SparseMemory::default(), 0x10000, 0x100000).unwrap();
    let base = p.virtual_alloc(0, 0x3000).unwrap();
    p.write(base, &[7]).unwrap();
    p.write(base + 0x1000, &[9]).unwrap();
    p.virtual_decommit(base + 0x1001, 1).unwrap();
    assert!(p.read(base + 0x1000, &mut [0]).is_err());
    assert!(p.virtual_alloc(base + 0x1000, 0x1000).is_err());
    p.virtual_alloc_flags(base + 0x1000, 0x1000, 0x1000, 4)
        .unwrap();
    let mut byte = [0];
    p.read(base + 0x1000, &mut byte).unwrap();
    assert_eq!(byte, [0]);
    p.read(base, &mut byte).unwrap();
    assert_eq!(byte, [7]);
    assert!(p.virtual_decommit(base + 0x2000, 0x2000).is_err());
    p.virtual_decommit(base, 0).unwrap();
    p.virtual_free(base).unwrap();
}
