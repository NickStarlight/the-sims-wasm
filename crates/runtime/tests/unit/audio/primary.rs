//! The movie player sizes its stream from the device's primary-buffer capabilities.
use super::*;
use crate::kernel::{manifest, resolve, Kernel};
use crate::SparseMemory;

#[test]
fn primary_capacity_supports_movie_audio_packets_and_secondary_wraps() {
    let mut p = Process::new(SparseMemory::default(), 0x1000, 0x1000000).unwrap();
    let mut kernel = Kernel::default();
    let bindings: Vec<_> = manifest().iter().filter(|api| api.dll == "dsound.dll").enumerate()
        .map(|(i, api)| serde_json::json!({"dll":"dsound.dll","name":api.name,"address":0x800000+i*4})).collect();
    kernel
        .install_modules(
            &serde_json::json!({"images":[],"nativeDlls":["dsound.dll"],"nativeBindings":bindings})
                .to_string(),
        )
        .unwrap();
    let call = |kernel: &mut Kernel, p: &mut Process<SparseMemory>, name, args: &[u32]| {
        let api = resolve("dsound", name).unwrap();
        if api.clock {
            kernel.set_monotonic_time(0.0).unwrap();
        }
        kernel.call(p, api.id, args).unwrap()
    };
    let out = p.heap_alloc(4).unwrap();
    assert_eq!(
        call(&mut kernel, &mut p, "DirectSoundCreate", &[0, out, 0]),
        0
    );
    let device = com::get(&p, out).unwrap();
    let desc = p.heap_alloc(20).unwrap();
    com::put(&mut p, desc, 20).unwrap();
    com::put(&mut p, desc + 4, 1).unwrap();
    assert_eq!(
        call(&mut kernel, &mut p, "DS_Device_3", &[device, desc, out, 0]),
        0
    );
    let primary = com::get(&p, out).unwrap();
    let caps = p.heap_alloc(20).unwrap();
    com::put(&mut p, caps, 20).unwrap();
    assert_eq!(
        call(&mut kernel, &mut p, "DS_Buffer_3", &[primary, caps]),
        0
    );
    let capacity = com::get(&p, caps + 8).unwrap();
    // The observed movie packets alternate 5824 and 5936 bytes. A 4096-byte
    // primary causes its derived secondary never to have space for even one.
    assert!(
        capacity >= 2 * 5936,
        "primary capacity {capacity} cannot buffer movie packets"
    );
    let format = p.heap_alloc(18).unwrap();
    p.write(
        format,
        &[
            1, 0, 2, 0, 0x22, 0x56, 0, 0, 0x88, 0x58, 1, 0, 4, 0, 16, 0, 0, 0,
        ],
    )
    .unwrap();
    for (offset, value) in [(4, 0), (8, capacity), (16, format)] {
        com::put(&mut p, desc + offset, value).unwrap();
    }
    assert_eq!(
        call(&mut kernel, &mut p, "DS_Device_3", &[device, desc, out, 0]),
        0
    );
    let secondary = com::get(&p, out).unwrap();
    let regions = p.heap_alloc(16).unwrap();
    assert_eq!(
        call(
            &mut kernel,
            &mut p,
            "DS_Buffer_11",
            &[
                secondary,
                capacity - 1000,
                5936,
                regions,
                regions + 4,
                regions + 8,
                regions + 12,
                0
            ]
        ),
        0
    );
    assert_eq!(com::get(&p, regions + 4).unwrap(), 1000);
    assert_eq!(com::get(&p, regions + 12).unwrap(), 4936);
}
