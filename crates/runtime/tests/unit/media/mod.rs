use super::*;
use crate::{
    kernel::{resolve, Kernel},
    SparseMemory,
};
fn process() -> Process<SparseMemory> {
    Process::new(SparseMemory::default(), 0x1000, 0x100000).unwrap()
}
#[test]
fn mmio_snapshots_cursor_seek_and_close_preserve_original_contract() {
    let mut p = process();
    let mut kernel = Kernel::default();
    kernel.add_file("sound.wav", b"RIFFdata").unwrap();
    let name = p.heap_alloc(16).unwrap();
    p.write(name, b"sound.wav\0").unwrap();
    let out = p.heap_alloc(16).unwrap();
    let call = |k: &mut Kernel, p: &mut Process<SparseMemory>, name, args: &[u32]| {
        let api = resolve("winmm", name).unwrap();
        assert!(api.media);
        k.call(p, api.id, args).unwrap()
    };
    let handle = call(&mut kernel, &mut p, "mmioOpenA", &[name, 0, 0]);
    assert_ne!(handle, 0);
    assert_eq!(call(&mut kernel, &mut p, "mmioRead", &[handle, out, 4]), 4);
    let mut bytes = [0; 4];
    p.read(out, &mut bytes).unwrap();
    assert_eq!(&bytes, b"RIFF");
    assert_eq!(
        call(
            &mut kernel,
            &mut p,
            "mmioSeek",
            &[handle, (-2i32) as u32, 2]
        ),
        6
    );
    assert_eq!(call(&mut kernel, &mut p, "mmioRead", &[handle, out, 8]), 2);
    assert_eq!(
        call(&mut kernel, &mut p, "mmioSeek", &[handle, 1, 2]),
        u32::MAX
    );
    assert_eq!(call(&mut kernel, &mut p, "mmioRead", &[handle, out, 8]), 0);
    assert_eq!(
        call(&mut kernel, &mut p, "mmioSetBuffer", &[handle, out, 16, 0]),
        0
    );
    assert_eq!(call(&mut kernel, &mut p, "mmioClose", &[handle, 0]), 0);
    assert_eq!(call(&mut kernel, &mut p, "mmioRead", &[handle, out, 4]), 0);
    assert_eq!(
        call(&mut kernel, &mut p, "mmioSeek", &[handle, 0, 0]),
        u32::MAX
    );
}
#[test]
fn timer_deadlines_advance_before_callback_and_allow_cancellation_reentry() {
    let mut p = process();
    let mut kernel = Kernel::default();
    let api = resolve("winmm", "timeSetEvent").unwrap();
    assert!(api.clock);
    kernel.set_monotonic_time(1000.0).unwrap();
    let id = kernel
        .call(&mut p, api.id, &[10, 1, 0x401000, 123, 1])
        .unwrap();
    assert!(kernel.next_media_timer(1009.0).unwrap().is_none());
    let callback = kernel.next_media_timer(1010.0).unwrap().unwrap();
    assert_eq!(callback.address, 0x401000);
    assert_eq!(callback.args, [id, 0, 123, 0, 0]);
    assert!(kernel.next_media_timer(1010.0).unwrap().is_none());
    assert_eq!(
        kernel.next_media_timer(1030.0).unwrap().unwrap().args[0],
        id
    );
    let kill = resolve("winmm", "timeKillEvent").unwrap();
    assert_eq!(kernel.call(&mut p, kill.id, &[id]).unwrap(), 0);
    assert!(kernel.next_media_timer(1030.0).unwrap().is_none());
    assert_eq!(kernel.call(&mut p, kill.id, &[id]).unwrap(), 97);
    kernel.set_monotonic_time(2000.0).unwrap();
    let once = kernel
        .call(&mut p, api.id, &[5, 1, 0x402000, 0, 0])
        .unwrap();
    assert_eq!(
        kernel.next_media_timer(2005.0).unwrap().unwrap().args[0],
        once
    );
    assert!(kernel.next_media_timer(2005.0).unwrap().is_none());
}
#[test]
fn timer_capacity_wrap_and_event_mode_match_original() {
    let mut media = Media::default();
    assert_eq!(media.create_timer(0, 1, 0, 0, 0), 0);
    for i in 0..16 {
        assert_ne!(media.create_timer(10, 0x401000, i, 0, u32::MAX - 5), 0);
    }
    assert_eq!(media.create_timer(10, 1, 0, 0, 0), 0);
    assert!(media.next_timer(u32::MAX).is_none());
    for _ in 0..16 {
        assert!(media.next_timer(4).is_some());
    }
    assert!(media.next_timer(4).is_none());
    media.create_timer(1, 77, 0, 0x10, 0);
    assert!(media.next_timer(1).is_none());
}
