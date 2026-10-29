use super::*;

#[test]
fn absent_wave_and_midi_devices_keep_the_original_contracts() {
    let mut kernel = Kernel::default();
    let mut process = Process::new(crate::SparseMemory::default(), 0x10000, 0x40000).unwrap();
    for (name, arity, expected) in [
        ("waveOutGetNumDevs", 0, 0),
        ("waveOutGetDevCapsA", 4, 2),
        ("waveOutOpen", 6, 6),
        ("waveOutClose", 1, 5),
        ("waveOutPrepareHeader", 3, 5),
        ("waveOutUnprepareHeader", 3, 5),
        ("waveOutWrite", 3, 5),
        ("waveOutPause", 1, 5),
        ("waveOutRestart", 1, 5),
        ("waveOutReset", 1, 5),
        ("waveOutGetPosition", 3, 5),
        ("waveOutSetVolume", 2, 5),
        ("waveOutGetVolume", 2, 5),
        ("midiOutGetNumDevs", 0, 0),
        ("midiOutGetDevCapsA", 4, 2),
        ("midiOutOpen", 6, 6),
        ("midiOutClose", 1, 5),
        ("midiOutShortMsg", 2, 5),
        ("midiStreamOpen", 6, 6),
        ("midiStreamClose", 1, 5),
        ("midiStreamOut", 3, 5),
        ("midiStreamPause", 1, 5),
        ("midiStreamRestart", 1, 5),
        ("midiStreamStop", 1, 5),
        ("midiStreamSetPosition", 3, 5),
    ] {
        let api = resolve("winmm.dll", name).unwrap();
        assert_eq!(api.arity, arity, "{name}");
        assert_eq!(api.stack_args, arity, "{name}");
        assert_eq!(
            kernel
                .call(&mut process, api.id, &vec![0; arity as usize])
                .unwrap(),
            expected,
            "{name}"
        );
    }
}
