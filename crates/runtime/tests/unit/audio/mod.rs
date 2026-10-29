//! PCM output regression contracts for the runtime audio owner.
use super::*;
use crate::SparseMemory;
fn process() -> Process<SparseMemory> {
    Process::new(SparseMemory::default(), 0x1000, 0x1000000).unwrap()
}
fn format() -> PcmFormat {
    PcmFormat {
        channels: 2,
        rate: 48000,
        bits: 16,
        align: 4,
    }
}
fn short_loop(audio: &mut Audio, process: &mut Process<SparseMemory>) -> u32 {
    let id = audio.create_buffer(process, 16, format(), 0).unwrap();
    let lock = audio.lock(id, 0, 16, 0).unwrap();
    let mut bytes = Vec::new();
    for sample in [4096i16, 8192, 16384, 24576] {
        bytes.extend(sample.to_le_bytes());
        bytes.extend((-sample).to_le_bytes());
    }
    process.write(lock.first_address, &bytes).unwrap();
    audio
        .unlock(process, id, (lock.first_address, 16), (0, 0))
        .unwrap();
    audio.play(id, true, 0.0).unwrap();
    id
}
#[test]
fn stereo_pcm_stops_to_silence_and_instances_do_not_share_buffers() {
    let mut process = process();
    let mut audio = Audio::default();
    let id = audio
        .create_buffer(&mut process, 16384, format(), 0)
        .unwrap();
    let lock = audio.lock(id, 0, 16384, 0).unwrap();
    let bytes: Vec<_> = (0..4096).flat_map(|_| [0, 32, 0, 192]).collect();
    process.write(lock.first_address, &bytes).unwrap();
    audio
        .unlock(&process, id, (lock.first_address, 16384), (0, 0))
        .unwrap();
    audio.play(id, true, 0.0).unwrap();
    let mixed = audio.mix(256, 48000.0, 0.0).unwrap();
    assert_eq!(mixed.left, vec![0.25; 256]);
    assert_eq!(mixed.right, vec![-0.5; 256]);
    audio.stop(id).unwrap();
    assert_eq!(audio.mix(256, 48000.0, 0.0).unwrap().left, vec![0.0; 256]);
    assert_eq!(
        Audio::default().mix(256, 48000.0, 0.0).unwrap().right,
        vec![0.0; 256]
    );
}
#[test]
fn short_loops_preserve_fractional_seams_restart_and_seek() {
    let mut process = process();
    let mut audio = Audio::default();
    let id = short_loop(&mut audio, &mut process);
    assert_eq!(
        audio.mix(13, 48000.0, 0.0).unwrap().left,
        (0..13)
            .map(|i| [0.125, 0.25, 0.5, 0.75][i % 4])
            .collect::<Vec<_>>()
    );
    assert_eq!(
        audio.mix(5, 48000.0, 0.0).unwrap().left,
        [0.25, 0.5, 0.75, 0.125, 0.25]
    );
    audio.stop(id).unwrap();
    audio.play(id, true, 0.0).unwrap();
    assert_eq!(
        audio.mix(8, 48000.0, 0.0).unwrap().left,
        [0.5, 0.75, 0.125, 0.25, 0.5, 0.75, 0.125, 0.25]
    );
    audio.seek(id, 4, 0.0).unwrap();
    assert_eq!(
        audio.mix(5, 48000.0, 0.0).unwrap().left,
        [0.25, 0.5, 0.75, 0.125, 0.25]
    );
    audio.stop(id).unwrap();
    short_loop(&mut audio, &mut process);
    assert_eq!(
        audio.mix(16, 96000.0, 0.0).unwrap().left,
        (0..16)
            .map(|i| [0.125, 0.1875, 0.25, 0.375, 0.5, 0.625, 0.75, 0.4375][i % 8])
            .collect::<Vec<_>>()
    );
}
#[test]
fn lock_wrap_and_invalid_guest_access_are_checked() {
    let mut process = process();
    let mut audio = Audio::default();
    let id = audio
        .create_buffer(&mut process, 4096, format(), 0)
        .unwrap();
    let lock = audio.lock(id, 4092, 8, 0).unwrap();
    assert_eq!((lock.first_bytes, lock.second_bytes), (4, 4));
    assert_eq!(lock.first_address - lock.second_address, 4092);
    assert!(audio
        .unlock(&process, id, (lock.second_address - 1, 4), (0, 0))
        .is_err());
    audio.remove_buffer(&mut process, id).unwrap();
    assert!(audio.play(id, true, 0.0).is_err());
    assert!(audio.mix(4097, 48000.0, 0.0).is_err());
}

#[test]
fn directsound_com_adapter_ports_vtables_primary_secondary_and_query_identity() {
    use crate::kernel::{manifest, resolve, Kernel};
    let mut process = process();
    let mut kernel = Kernel::default();
    let bindings: Vec<_> = manifest().iter().filter(|api| api.dll == "dsound.dll").enumerate()
        .map(|(i,api)| serde_json::json!({"dll":"dsound.dll","name":api.name,"address":0x800000+i*4})).collect();
    kernel
        .install_modules(
            &serde_json::json!({"images":[],"nativeDlls":["dsound.dll"],"nativeBindings":bindings})
                .to_string(),
        )
        .unwrap();
    let call = |kernel: &mut Kernel, process: &mut Process<SparseMemory>, name, args: &[u32]| {
        let api = resolve("dsound", name).unwrap();
        if api.clock {
            kernel.set_monotonic_time(0.0).unwrap();
        }
        kernel.call(process, api.id, args).unwrap()
    };
    let out = process.heap_alloc(4).unwrap();
    assert_eq!(
        call(&mut kernel, &mut process, "DirectSoundCreate", &[0, out, 0]),
        0
    );
    let device = com::get(&process, out).unwrap();
    let table = com::get(&process, device).unwrap();
    assert_ne!(com::get(&process, table + 3 * 4).unwrap(), 0);
    let desc = process.heap_alloc(20).unwrap();
    let fmt = process.heap_alloc(18).unwrap();
    let format_bytes = [
        1, 0, 2, 0, 0x80, 0xbb, 0, 0, 0, 0xee, 2, 0, 4, 0, 16, 0, 0, 0,
    ];
    process.write(fmt, &format_bytes).unwrap();
    for (offset, value) in [(0, 20), (4, 0), (8, 16), (16, fmt)] {
        com::put(&mut process, desc + offset, value).unwrap();
    }
    assert_eq!(
        call(
            &mut kernel,
            &mut process,
            "DS_Device_3",
            &[device, desc, out, 0]
        ),
        0
    );
    let buffer = com::get(&process, out).unwrap();
    let p = process.heap_alloc(16).unwrap();
    assert_eq!(
        call(
            &mut kernel,
            &mut process,
            "DS_Buffer_11",
            &[buffer, 0, 16, p, p + 4, p + 8, p + 12, 0]
        ),
        0
    );
    let address = com::get(&process, p).unwrap();
    let bytes: Vec<_> = (0..4).flat_map(|_| [0, 32, 0, 192]).collect();
    process.write(address, &bytes).unwrap();
    assert_eq!(
        call(
            &mut kernel,
            &mut process,
            "DS_Buffer_19",
            &[buffer, address, 16, 0, 0]
        ),
        0
    );
    assert_eq!(
        call(
            &mut kernel,
            &mut process,
            "DS_Buffer_12",
            &[buffer, 0, 0, 1]
        ),
        0
    );
    let block = kernel.mix_audio(8, 48000.0, 0.0).unwrap();
    assert_eq!(block.left, vec![0.25; 8]);
    assert_eq!(block.right, vec![-0.5; 8]);
    let iid = process.heap_alloc(16).unwrap();
    process.write(iid, &[0; 16]).unwrap();
    assert_eq!(
        call(
            &mut kernel,
            &mut process,
            "DS_Buffer_0",
            &[buffer, iid, out]
        ),
        0x80004002
    );
    assert_eq!(com::get(&process, out).unwrap(), 0);
    com::put(&mut process, iid + 8, 0xc0).unwrap();
    com::put(&mut process, iid + 12, 0x46000000).unwrap();
    assert_eq!(
        call(
            &mut kernel,
            &mut process,
            "DS_Buffer_0",
            &[buffer, iid, out]
        ),
        0
    );
    assert_eq!(com::get(&process, out).unwrap(), buffer);
    com::put(&mut process, desc + 4, 1).unwrap();
    com::put(&mut process, desc + 16, 0).unwrap();
    assert_eq!(
        call(
            &mut kernel,
            &mut process,
            "DS_Device_3",
            &[device, desc, out, 0]
        ),
        0
    );
    let primary = com::get(&process, out).unwrap();
    assert_eq!(
        call(
            &mut kernel,
            &mut process,
            "DS_Buffer_12",
            &[primary, 0, 0, 0]
        ),
        com::INVALID
    );
    assert_eq!(
        call(&mut kernel, &mut process, "DS_Buffer_14", &[primary, fmt]),
        0x88780046
    );
    call(&mut kernel, &mut process, "DS_Device_6", &[device, 0, 2]);
    assert_eq!(
        call(&mut kernel, &mut process, "DS_Buffer_14", &[primary, fmt]),
        0
    );
}

#[test]
fn timed_directsound_methods_request_real_samples() {
    for name in ["DS_Buffer_4", "DS_Buffer_9", "DS_Buffer_12", "DS_Buffer_13"] {
        assert!(
            crate::kernel::resolve("dsound", name).unwrap().clock,
            "{name}"
        );
    }
    let mut p = process();
    let mut audio = Audio::default();
    let id = short_loop(&mut audio, &mut p);
    audio.play(id, false, 1000.0).unwrap();
    assert!(audio.status(id, 1000.01).unwrap().0);
    assert_eq!(audio.mix(1, 48000.0, 1000.01).unwrap().left, [0.125]);
    assert!(!audio.status(id, 2000.0).unwrap().0);
}

#[test]
fn streaming_unlock_replaces_native_offsets_instead_of_appending() {
    let mut p = process();
    let mut audio = Audio::default();
    let id = audio.create_buffer(&mut p, 16, format(), 0).unwrap();
    let locked = audio.lock(id, 0, 16, 0).unwrap();
    // Streaming engines clear their whole circular buffer before refilling it.
    p.write(locked.first_address, &[0; 16]).unwrap();
    audio
        .unlock(&p, id, (locked.first_address, 16), (0, 0))
        .unwrap();
    audio.play(id, true, 0.0).unwrap();
    let sample = [0, 32, 0, 192];
    p.write(locked.first_address + 8, &sample).unwrap();
    audio
        .unlock(&p, id, (locked.first_address + 8, 4), (0, 0))
        .unwrap();
    assert_eq!(
        audio.mix(8, 48000.0, 0.0).unwrap().left,
        [0.0, 0.0, 0.25, 0.0, 0.0, 0.0, 0.25, 0.0]
    );
    // Out-of-order rewrites replace the same physical frame, even after loops.
    p.write(locked.first_address + 8, &[0, 64, 0, 192]).unwrap();
    audio
        .unlock(&p, id, (locked.first_address + 8, 4), (0, 0))
        .unwrap();
    assert_eq!(
        audio.mix(4, 48000.0, 0.0).unwrap().left,
        [0.0, 0.0, 0.5, 0.0]
    );
}

#[test]
fn duplicate_can_play_existing_pcm_without_another_unlock() {
    let mut p = process();
    let mut audio = Audio::default();
    let original = short_loop(&mut audio, &mut p);
    audio.stop(original).unwrap();
    let duplicate = audio.duplicate_buffer(&mut p, original).unwrap();
    audio.play(duplicate, true, 0.0).unwrap();
    assert_eq!(
        audio.mix(5, 48000.0, 0.0).unwrap().left,
        [0.125, 0.25, 0.5, 0.75, 0.125]
    );
}
