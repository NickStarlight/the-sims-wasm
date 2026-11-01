use super::*;
use crate::SparseMemory;
fn setup() -> (Kernel, Process<SparseMemory>, u32) {
    let mut kernel = Kernel::default();
    let bindings: Vec<_> = manifest::APIS
        .iter()
        .filter(|api| api.dll == "quartz.dll")
        .enumerate()
        .map(|(i, api)| serde_json::json!({"dll":api.dll,"name":api.name,"address":0x800000+i*4}))
        .collect();
    kernel
        .install_modules(
            &serde_json::json!({"images":[],"nativeDlls":["quartz.dll"],"nativeBindings":bindings})
                .to_string(),
        )
        .unwrap();
    let mut p = Process::new(SparseMemory::default(), 0x10000, 0x100000).unwrap();
    let data = p.heap_alloc(64).unwrap();
    p.write(data, &FILTER_GRAPH).unwrap();
    p.write(data + 16, &iid(0x56a868a9)).unwrap();
    assert_eq!(
        kernel
            .create_graph(&mut p, &[data, 0, 3, data + 16, data + 32])
            .unwrap(),
        0
    );
    let mut bytes = [0; 4];
    p.read(data + 32, &mut bytes).unwrap();
    (kernel, p, u32::from_le_bytes(bytes))
}
#[test]
fn interfaces_share_identity_and_release_graph_resources() {
    let (mut k, mut p, graph) = setup();
    let data = p.heap_alloc(32).unwrap();
    p.write(data, &iid(0x56a868b1)).unwrap();
    assert_eq!(
        k.graph_call(&mut p, "DG_Query", &[graph, data, data + 16])
            .unwrap(),
        0
    );
    let mut bytes = [0; 4];
    p.read(data + 16, &mut bytes).unwrap();
    let control = u32::from_le_bytes(bytes);
    assert_ne!(graph, control);
    assert_eq!(
        k.graph_call(&mut p, "DG_Run", &[control]).unwrap(),
        0x80040209
    );
    p.write(data, &UNKNOWN).unwrap();
    k.graph_call(&mut p, "DG_Query", &[control, data, data + 16])
        .unwrap();
    p.read(data + 16, &mut bytes).unwrap();
    assert_eq!(u32::from_le_bytes(bytes), graph);
    assert_eq!(k.graph_call(&mut p, "DG_Release", &[control]).unwrap(), 2);
    assert_eq!(k.graph_call(&mut p, "DG_Release", &[graph]).unwrap(), 1);
    assert_eq!(k.graph_call(&mut p, "DG_Release", &[graph]).unwrap(), 0);
    assert!(k.directshow.graphs.is_empty());
    assert!(k.directshow.objects.is_empty());
}
#[test]
fn mp3_playback_pauses_resumes_and_reports_completion_once() {
    let (mut k, mut p, graph) = setup();
    k.add_file("tone.mp3", include_bytes!("../../fixtures/stereo-tone.mp3"))
        .unwrap();
    let data = p.heap_alloc(64).unwrap();
    let name: Vec<_> = "tone.mp3\0"
        .encode_utf16()
        .flat_map(u16::to_le_bytes)
        .collect();
    p.write(data, &name).unwrap();
    assert_eq!(
        k.graph_call(&mut p, "DG_Render", &[graph, data, 0])
            .unwrap(),
        0
    );
    assert_eq!(k.graph_call(&mut p, "DG_Run", &[graph]).unwrap(), 0);
    let block = k.mix_audio(4096, 48000.0, 0.0).unwrap();
    assert!(block.left.iter().any(|x| x.abs() > 0.05));
    k.graph_call(&mut p, "DG_Pause", &[graph]).unwrap();
    assert!(k
        .mix_audio(4096, 48000.0, 100.0)
        .unwrap()
        .left
        .iter()
        .all(|&x| x == 0.0));
    k.graph_call(&mut p, "DG_Run", &[graph]).unwrap();
    for n in 0..8 {
        k.mix_audio(4096, 48000.0, 200.0 + n as f64 * 100.0)
            .unwrap();
    }
    assert_eq!(
        k.graph_call(&mut p, "DG_Event", &[graph, data, data + 4, data + 8, 0])
            .unwrap(),
        0
    );
    let mut bytes = [0; 4];
    p.read(data, &mut bytes).unwrap();
    assert_eq!(u32::from_le_bytes(bytes), 1);
    assert_eq!(
        k.graph_call(&mut p, "DG_Event", &[graph, data, data + 4, data + 8, 0])
            .unwrap(),
        0x80004004
    );
    assert!(k
        .mix_audio(4096, 48000.0, 1100.0)
        .unwrap()
        .left
        .iter()
        .all(|&x| x == 0.0));
}

#[test]
fn missing_and_invalid_media_fail_without_starting_playback() {
    let (mut k, mut p, graph) = setup();
    let path = p.heap_alloc(64).unwrap();
    let bytes: Vec<_> = "missing.mp3\0"
        .encode_utf16()
        .flat_map(u16::to_le_bytes)
        .collect();
    p.write(path, &bytes).unwrap();
    assert_eq!(
        k.graph_call(&mut p, "DG_Render", &[graph, path, 0])
            .unwrap(),
        0x80070002
    );
    k.add_file("missing.mp3", b"not an mp3").unwrap();
    assert_eq!(
        k.graph_call(&mut p, "DG_Render", &[graph, path, 0])
            .unwrap(),
        0x80040265
    );
    assert_eq!(
        k.graph_call(&mut p, "DG_Volume", &[graph, 1]).unwrap(),
        INVALID
    );
    assert_eq!(
        k.graph_call(&mut p, "DG_Balance", &[graph, 10001]).unwrap(),
        INVALID
    );
    assert_eq!(
        k.graph_call(&mut p, "DG_Event", &[graph, path, path + 4, path + 8, 0])
            .unwrap(),
        0x80004004
    );
}
