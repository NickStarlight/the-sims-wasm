//! Behavioral checks for graph lifecycle, event delivery, and fault isolation.
use super::*;
use crate::{user::Action, SparseMemory};
use std::{cell::Cell, rc::Rc};

const TONE: &[u8] = include_bytes!("../../../fixtures/stereo-tone.mp3");
const UNTRIMMED: &[u8] = include_bytes!("../../../fixtures/stereo-untrimmed.mp3");
const DATA: u32 = 0x1000;

fn kernel() -> Kernel {
    let mut k = Kernel::default();
    let bindings: Vec<_> = manifest::APIS
        .iter()
        .filter(|api| api.dll == "quartz.dll")
        .enumerate()
        .map(|(i, api)| serde_json::json!({"dll":api.dll,"name":api.name,"address":0x800000+i*4}))
        .collect();
    k.install_modules(
        &serde_json::json!({"images":[],"nativeDlls":["quartz.dll"],"nativeBindings":bindings})
            .to_string(),
    )
    .unwrap();
    k
}
fn new_graph<M: GuestMemory>(k: &mut Kernel, p: &mut Process<M>) -> u32 {
    p.write(DATA, &FILTER_GRAPH).unwrap();
    p.write(DATA + 16, &iid(0x56a868a9)).unwrap();
    assert_eq!(
        k.create_graph(p, &[DATA, 0, 3, DATA + 16, DATA + 32])
            .unwrap(),
        0
    );
    word(p, DATA + 32)
}
fn word<M: GuestMemory>(p: &Process<M>, address: u32) -> u32 {
    let mut bytes = [0; 4];
    p.read(address, &mut bytes).unwrap();
    u32::from_le_bytes(bytes)
}
fn fixture() -> (Kernel, Process<SparseMemory>, u32) {
    let mut k = kernel();
    let mut p = Process::new(SparseMemory::default(), 0x10000, 0x100000).unwrap();
    p.map(DATA, &[0; 1024], true).unwrap();
    let graph = new_graph(&mut k, &mut p);
    (k, p, graph)
}
fn load(k: &mut Kernel, p: &mut Process<SparseMemory>, graph: u32, name: &str, bytes: &[u8]) {
    k.add_file(name, bytes).unwrap();
    let wide: Vec<_> = format!("{name}\0")
        .encode_utf16()
        .flat_map(u16::to_le_bytes)
        .collect();
    p.write(DATA + 128, &wide).unwrap();
    assert_eq!(
        k.graph_call(p, "DG_Render", &[graph, DATA + 128, 0])
            .unwrap(),
        0
    );
}
fn call(k: &mut Kernel, p: &mut Process<SparseMemory>, name: &str, args: &[u32]) -> u32 {
    k.graph_call(p, name, args).unwrap()
}
fn drain(k: &mut Kernel) {
    for n in 0..8 {
        k.mix_audio(4096, 48000.0, n as f64 * 100.0).unwrap();
    }
}
fn event(k: &mut Kernel, p: &mut Process<SparseMemory>, graph: u32) -> u32 {
    call(
        k,
        p,
        "DG_Event",
        &[graph, DATA + 256, DATA + 260, DATA + 264, 0],
    )
}
fn window(k: &mut Kernel, p: &mut Process<SparseMemory>) -> u32 {
    p.write(DATA + 400, b"GraphTest\0").unwrap();
    let fields = [0u32, 0x401000, 0, 0, 0x400000, 0, 0, 0, 0, DATA + 400];
    p.write(DATA + 320, fields.map(u32::to_le_bytes).as_flattened())
        .unwrap();
    assert_ne!(k.user.call(p, "RegisterClassA", &[DATA + 320]).unwrap(), 0);
    let mut action = k
        .user
        .begin(
            p,
            "CreateWindowExA",
            &[0, 1, 0, 0, 0, 0, 800, 600, 0, 0, 0x400000, 0],
        )
        .unwrap();
    loop {
        match action {
            Action::Return(hwnd) => {
                assert_ne!(hwnd, 0);
                return hwnd;
            }
            Action::Invoke { args, next, .. } => {
                action = k.user.resume(next, u32::from(args[1] == 0x81)).unwrap();
            }
        }
    }
}

#[test]
fn eof_remains_running_and_pause_run_notifies_again_without_replaying() {
    let (mut k, mut p, graph) = fixture();
    load(&mut k, &mut p, graph, "tone.mp3", TONE);
    call(&mut k, &mut p, "DG_Run", &[graph]);
    drain(&mut k);
    call(&mut k, &mut p, "DG_State", &[graph, 0, DATA + 280]);
    assert_eq!(word(&p, DATA + 280), 2);
    assert_eq!(event(&mut k, &mut p, graph), 0);
    assert_eq!(word(&p, DATA + 256), 1);
    assert_eq!(event(&mut k, &mut p, graph), 0x80004004);
    call(&mut k, &mut p, "DG_Run", &[graph]);
    drain(&mut k);
    assert_eq!(
        event(&mut k, &mut p, graph),
        0x80004004,
        "Run on an already-running graph must not repeat EOF"
    );
    call(&mut k, &mut p, "DG_Pause", &[graph]);
    call(&mut k, &mut p, "DG_Run", &[graph]);
    let block = k.mix_audio(4096, 48000.0, 1000.0).unwrap();
    assert!(block.left.iter().chain(&block.right).all(|&x| x == 0.0));
    assert_eq!(event(&mut k, &mut p, graph), 0);
    assert_eq!(word(&p, DATA + 256), 1);
}

#[test]
fn notification_flags_gate_window_messages_but_not_pending_events() {
    let (mut k, mut p, graph) = fixture();
    let hwnd = window(&mut k, &mut p);
    load(&mut k, &mut p, graph, "tone.mp3", TONE);
    call(
        &mut k,
        &mut p,
        "DG_Notify",
        &[graph, hwnd, 0xc00, 0x12345678],
    );
    call(&mut k, &mut p, "DG_Flags", &[graph, 1]);
    call(&mut k, &mut p, "DG_Run", &[graph]);
    drain(&mut k);
    assert_eq!(
        k.user
            .call(&mut p, "PeekMessageA", &[DATA + 448, hwnd, 0xc00, 0xc00, 1])
            .unwrap(),
        0
    );
    assert_eq!(event(&mut k, &mut p, graph), 0);
    call(&mut k, &mut p, "DG_Flags", &[graph, 0]);
    call(&mut k, &mut p, "DG_Pause", &[graph]);
    call(&mut k, &mut p, "DG_Run", &[graph]);
    drain(&mut k);
    assert_eq!(
        k.user
            .call(&mut p, "PeekMessageA", &[DATA + 448, hwnd, 0xc00, 0xc00, 1])
            .unwrap(),
        1
    );
    assert_eq!(word(&p, DATA + 456), 0, "notification wParam");
    assert_eq!(
        word(&p, DATA + 460),
        0x12345678,
        "notification instance data"
    );
    assert_eq!(
        k.user
            .call(&mut p, "PeekMessageA", &[DATA + 448, hwnd, 0xc00, 0xc00, 1])
            .unwrap(),
        0
    );
}

#[test]
fn volume_balance_and_release_do_not_affect_other_graphs() {
    let (mut k, mut p, graph) = fixture();
    let other = new_graph(&mut k, &mut p);
    load(&mut k, &mut p, graph, "first.mp3", TONE);
    load(&mut k, &mut p, other, "second.mp3", TONE);
    call(&mut k, &mut p, "DG_Volume", &[graph, (-10000i32) as u32]);
    call(&mut k, &mut p, "DG_Balance", &[other, 10000]);
    call(&mut k, &mut p, "DG_Run", &[graph]);
    call(&mut k, &mut p, "DG_Run", &[other]);
    let block = k.mix_audio(4096, 48000.0, 0.0).unwrap();
    assert!(block.left.iter().all(|&x| x == 0.0));
    assert!(block.right.iter().any(|x| x.abs() > 0.05));
    let interfaces = k.directshow.graphs[&graph].interfaces;
    assert_eq!(call(&mut k, &mut p, "DG_Release", &[graph]), 0);
    for address in interfaces {
        assert!(p.heap_size(address).is_err());
    }
    let block = k.mix_audio(4096, 48000.0, 100.0).unwrap();
    assert!(block.right.iter().any(|x| x.abs() > 0.05));
    assert_eq!(call(&mut k, &mut p, "DG_Release", &[other]), 0);
    assert!(k
        .mix_audio(4096, 48000.0, 200.0)
        .unwrap()
        .right
        .iter()
        .all(|&x| x == 0.0));
}

#[test]
fn corrupt_packets_report_error_without_breaking_other_audio() {
    let (mut k, mut p, graph) = fixture();
    // Valid initial frames allow RenderFile to succeed. Follow them with more
    // corrupt frames than one callback permits; headers remain valid MPEG1 L3.
    let mut mixed = UNTRIMMED.to_vec();
    let mut corrupt = UNTRIMMED[UNTRIMMED.len() - 418..].to_vec();
    corrupt[4..].fill(0xff);
    for _ in 0..70 {
        mixed.extend_from_slice(&corrupt);
    }
    load(&mut k, &mut p, graph, "corrupt.mp3", &mixed);
    call(&mut k, &mut p, "DG_Run", &[graph]);
    drain(&mut k);
    assert_eq!(event(&mut k, &mut p, graph), 0);
    assert_eq!(word(&p, DATA + 256), 3, "EC_ERRORABORT");
    assert_ne!(word(&p, DATA + 260) & 0x80000000, 0);
    assert!(k.directshow.graphs[&graph].stream.is_none());
    assert_eq!(event(&mut k, &mut p, graph), 0x80004004);
    let other = new_graph(&mut k, &mut p);
    load(&mut k, &mut p, other, "good.mp3", TONE);
    call(&mut k, &mut p, "DG_Run", &[other]);
    assert!(k
        .mix_audio(4096, 48000.0, 2000.0)
        .unwrap()
        .left
        .iter()
        .any(|x| x.abs() > 0.05));
}

#[test]
fn invalid_enumerator_output_does_not_retain_a_reference() {
    let (mut k, mut p, graph) = fixture();
    assert!(k
        .graph_call(&mut p, "DG_Enum", &[graph, 0xdeadbeef])
        .is_err());
    assert_eq!(call(&mut k, &mut p, "DG_Release", &[graph]), 0);
    assert!(k.directshow.graphs.is_empty());
}

struct FailingMemory {
    memory: SparseMemory,
    remaining: Rc<Cell<Option<usize>>>,
}
impl GuestMemory for FailingMemory {
    fn ensure(&mut self, end: u64) -> Result<()> {
        if let Some(count) = self.remaining.get() {
            if count == 0 {
                return Err(Error::OutOfMemory);
            }
            self.remaining.set(Some(count - 1));
        }
        self.memory.ensure(end)
    }
    fn read(&self, a: u32, b: &mut [u8]) -> Result<()> {
        self.memory.read(a, b)
    }
    fn write(&mut self, a: u32, b: &[u8]) -> Result<()> {
        self.memory.write(a, b)
    }
    fn fill(&mut self, a: u32, n: u32, v: u8) -> Result<()> {
        self.memory.fill(a, n, v)
    }
    fn copy(&mut self, d: u32, s: u32, n: u32) -> Result<()> {
        self.memory.copy(d, s, n)
    }
}
#[test]
fn graph_allocation_failure_frees_partially_created_interface_cells() {
    let remaining = Rc::new(Cell::new(None));
    let mut p = Process::new(
        FailingMemory {
            memory: SparseMemory::default(),
            remaining: remaining.clone(),
        },
        0x10000,
        0x100000,
    )
    .unwrap();
    p.map(DATA, &[0; 1024], true).unwrap();
    let mut k = kernel();
    new_graph(&mut k, &mut p); // populate the shared vtable cache first
    let before: Vec<_> = p.regions().collect();
    remaining.set(Some(2));
    assert!(k
        .create_graph(&mut p, &[DATA, 0, 3, DATA + 16, DATA + 32])
        .is_err());
    let after: Vec<_> = p.regions().collect();
    let allocated = &after[before.len()..];
    assert_eq!(allocated.len(), 2);
    for region in allocated {
        assert!(p.heap_size(region.address).is_err());
    }
    remaining.set(None);
    let reused = [p.heap_alloc(4).unwrap(), p.heap_alloc(4).unwrap()];
    assert!(allocated
        .iter()
        .all(|region| reused.contains(&region.address)));
    assert_eq!(k.directshow.graphs.len(), 1);
    assert_eq!(k.directshow.objects.len(), 5);
}
