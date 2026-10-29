//! Bulk operations on a guest WebAssembly.Memory owned by the browser worker.
//! The Rust module's own memory and allocator never contain guest addresses.
use crate::{Error, GuestMemory, Process, Result, U32Update};
use js_sys::{DataView, Function, Reflect, Uint8Array, WebAssembly::Memory};
use std::cell::{Ref, RefCell};
use wasm_bindgen::prelude::*;

struct BrowserMemory {
    memory: Memory,
    scalar: RefCell<Option<ScalarView>>,
    linked: Option<LinkedMemory>,
}

struct LinkedMemory(u32);

#[wasm_bindgen(raw_module = "./guest-memory.js")]
extern "C" {
    #[wasm_bindgen(catch, js_name = registerGuestMemory)]
    fn register_guest_memory(
        memory: &Memory,
        runtime: &JsValue,
    ) -> std::result::Result<f64, JsValue>;
    #[wasm_bindgen(catch, js_name = releaseGuestMemory)]
    fn release_guest_memory(handle: u32) -> std::result::Result<(), JsValue>;
}

// Unlike a wasm-bindgen function shim, this explicit primitive ABI imports the
// helper's actual Wasm export. The standard generated web loader passes it through.
#[link(wasm_import_module = "./guest-memory.js")]
extern "C" {
    #[link_name = "guestMemoryAccess"]
    fn guest_memory_access(handle: u32, operation: u32, a: u32, b: u32, c: u32, d: u32) -> f64;
}

impl LinkedMemory {
    fn access(&self, operation: u32, a: u32, b: u32, c: u32, d: u32) -> f64 {
        // Registration links this slot to this guest and wasm_bindgen::memory().
        // Both ranges are checked with 64-bit arithmetic before any access. Bulk
        // callers pass pointers only to live Rust slices in that runtime memory.
        unsafe { guest_memory_access(self.0, operation, a, b, c, d) }
    }
}

impl Drop for LinkedMemory {
    fn drop(&mut self) {
        // Release the table's function reference, including on constructor errors.
        let _ = release_guest_memory(self.0);
    }
}

fn linked_result(status: f64) -> Result<()> {
    if status < 0.0 {
        Err(Error::Unmapped)
    } else {
        Ok(())
    }
}
struct ScalarView {
    buffer: JsValue,
    length: u64,
    view: DataView,
}

#[wasm_bindgen(raw_module = "./memory-ops.js")]
extern "C" {
    #[wasm_bindgen(catch, js_name = updateU32)]
    fn update_u32_memory(
        memory: &Memory,
        address: u32,
        operation: u32,
        value: u32,
        comparand: u32,
    ) -> std::result::Result<f64, JsValue>;
}

#[wasm_bindgen]
extern "C" {
    // ArrayBuffer and SharedArrayBuffer both expose this getter. Use f64 so a
    // 4 GiB memory length cannot wrap to zero at the wasm32 boundary.
    type GuestBuffer;
    #[wasm_bindgen(method, getter, js_name = byteLength)]
    fn byte_length(this: &GuestBuffer) -> f64;
}

fn host(error: JsValue) -> Error {
    Error::Host(format!("{error:?}"))
}
fn js(error: Error) -> JsError {
    JsError::new(&error.to_string())
}

impl BrowserMemory {
    fn new(memory: Memory) -> Result<Self> {
        let handle = register_guest_memory(&memory, &wasm_bindgen::memory()).map_err(host)?;
        Ok(Self {
            memory,
            scalar: RefCell::new(None),
            linked: (handle >= 0.0).then(|| LinkedMemory(handle as u32)),
        })
    }

    fn length(&self) -> Result<u64> {
        if let Some(linked) = &self.linked {
            return Ok(linked.access(7, 0, 0, 0, 0) as u64);
        }
        Ok(self
            .memory
            .buffer()
            .unchecked_ref::<GuestBuffer>()
            .byte_length() as u64)
    }

    fn scalar_view(&self, address: u32, size: usize) -> Result<Option<Ref<'_, ScalarView>>> {
        let buffer = self.memory.buffer();
        let mut cached = self.scalar.borrow_mut();
        if cached
            .as_ref()
            .is_none_or(|current| current.buffer != buffer)
        {
            let length = buffer.unchecked_ref::<GuestBuffer>().byte_length() as u64;
            // The explicit DataView length is usize on wasm32. Keep the general
            // typed-array path for a complete 4 GiB memory instead of wrapping.
            if length > u64::from(u32::MAX) {
                return Ok(None);
            }
            *cached = Some(ScalarView {
                view: DataView::new(buffer.unchecked_ref(), 0, length as usize),
                buffer,
                length,
            });
        }
        if u64::from(address) + size as u64 > cached.as_ref().unwrap().length {
            return Err(Error::Unmapped);
        }
        drop(cached);
        Ok(Some(Ref::map(self.scalar.borrow(), |cached| {
            cached.as_ref().unwrap()
        })))
    }

    fn view(&self, address: u32, size: u32) -> Result<Uint8Array> {
        let buffer = self.memory.buffer();
        if u64::from(address) + u64::from(size)
            > buffer.unchecked_ref::<GuestBuffer>().byte_length() as u64
        {
            return Err(Error::Unmapped);
        }
        // Refresh the view after every memory.grow; old ArrayBuffers can be detached.
        Ok(Uint8Array::new_with_byte_offset_and_length(
            &buffer, address, size,
        ))
    }
}

impl GuestMemory for BrowserMemory {
    fn ensure(&mut self, end: u64) -> Result<()> {
        let length = self.length()?;
        if end <= length {
            return Ok(());
        }
        if end > 1u64 << 32 {
            return Err(Error::OutOfMemory);
        }
        // call1 catches RangeError from memory.grow, preserving process metadata on OOM.
        let grow = Reflect::get(self.memory.as_ref(), &JsValue::from_str("grow"))
            .map_err(host)?
            .dyn_into::<Function>()
            .map_err(host)?;
        grow.call1(
            self.memory.as_ref(),
            &JsValue::from_f64(((end - length).div_ceil(65536)) as f64),
        )
        .map_err(|error| {
            if error.is_instance_of::<js_sys::RangeError>() {
                Error::OutOfMemory
            } else {
                host(error)
            }
        })?;
        Ok(())
    }

    fn read(&self, address: u32, bytes: &mut [u8]) -> Result<()> {
        if let Some(linked) = &self.linked {
            if matches!(bytes.len(), 1 | 2 | 4) {
                let value = linked.access(0, address, bytes.len() as u32, 0, 0);
                linked_result(value)?;
                bytes.copy_from_slice(&(value as u32).to_le_bytes()[..bytes.len()]);
                return Ok(());
            }
            return linked_result(linked.access(
                2,
                address,
                bytes.as_mut_ptr() as u32,
                u32::try_from(bytes.len()).map_err(|_| Error::InvalidRange)?,
                0,
            ));
        }
        if matches!(bytes.len(), 1 | 2 | 4) {
            if let Some(current) = self.scalar_view(address, bytes.len())? {
                let offset = address as usize;
                match bytes.len() {
                    1 => bytes[0] = current.view.get_uint8(offset),
                    2 => bytes.copy_from_slice(
                        &current.view.get_uint16_endian(offset, true).to_le_bytes(),
                    ),
                    _ => bytes.copy_from_slice(
                        &current.view.get_uint32_endian(offset, true).to_le_bytes(),
                    ),
                }
                return Ok(());
            }
        }
        self.view(
            address,
            u32::try_from(bytes.len()).map_err(|_| Error::InvalidRange)?,
        )?
        .copy_to(bytes);
        Ok(())
    }

    fn write(&mut self, address: u32, bytes: &[u8]) -> Result<()> {
        if let Some(linked) = &self.linked {
            if matches!(bytes.len(), 1 | 2 | 4) {
                let mut value = [0; 4];
                value[..bytes.len()].copy_from_slice(bytes);
                return linked_result(linked.access(
                    1,
                    address,
                    bytes.len() as u32,
                    u32::from_le_bytes(value),
                    0,
                ));
            }
            return linked_result(linked.access(
                3,
                address,
                bytes.as_ptr() as u32,
                u32::try_from(bytes.len()).map_err(|_| Error::InvalidRange)?,
                0,
            ));
        }
        if matches!(bytes.len(), 1 | 2 | 4) {
            if let Some(current) = self.scalar_view(address, bytes.len())? {
                let offset = address as usize;
                match bytes.len() {
                    1 => current.view.set_uint8(offset, bytes[0]),
                    2 => current.view.set_uint16_endian(
                        offset,
                        u16::from_le_bytes(bytes.try_into().unwrap()),
                        true,
                    ),
                    _ => current.view.set_uint32_endian(
                        offset,
                        u32::from_le_bytes(bytes.try_into().unwrap()),
                        true,
                    ),
                }
                return Ok(());
            }
        }
        self.view(
            address,
            u32::try_from(bytes.len()).map_err(|_| Error::InvalidRange)?,
        )?
        .copy_from(bytes);
        Ok(())
    }

    fn fill(&mut self, address: u32, size: u32, value: u8) -> Result<()> {
        if let Some(linked) = &self.linked {
            return linked_result(linked.access(4, address, size, u32::from(value), 0));
        }
        self.view(address, size)?.fill(value, 0, size);
        Ok(())
    }

    fn update_u32(&mut self, address: u32, operation: U32Update) -> Result<u32> {
        let (kind, value, comparand) = match operation {
            U32Update::Add(value) => (0, value, 0),
            U32Update::Exchange(value) => (1, value, 0),
            U32Update::CompareExchange {
                exchange,
                comparand,
            } => (2, exchange, comparand),
        };
        let previous = if let Some(linked) = &self.linked {
            linked.access(6, address, kind, value, comparand)
        } else {
            update_u32_memory(&self.memory, address, kind, value, comparand).map_err(host)?
        };
        // A negative sentinel distinguishes backing bounds errors from every DWORD value.
        if previous < 0.0 {
            return Err(Error::Unmapped);
        }
        Ok(previous as u32)
    }

    fn copy(&mut self, destination: u32, source: u32, size: u32) -> Result<()> {
        if size == 0 {
            return Ok(());
        }
        if let Some(linked) = &self.linked {
            return linked_result(linked.access(5, destination, source, size, 0));
        }
        let source = self.view(source, size)?;
        self.view(destination, size)?.set(&source, 0);
        Ok(())
    }
}

/// One explicit process per guest memory. free() drops metadata, not the caller's memory.
#[wasm_bindgen(js_name = Process)]
pub struct BrowserProcess {
    process: Process<BrowserMemory>,
    kernel: crate::kernel::Kernel,
    sync_call_error: Option<JsValue>,
}

#[wasm_bindgen(js_class = Process)]
impl BrowserProcess {
    #[wasm_bindgen(js_name = nextMediaTimer)]
    pub fn next_media_timer(
        &mut self,
        now_ms: f64,
    ) -> std::result::Result<Option<String>, JsError> {
        self.kernel
            .next_media_timer(now_ms)
            .and_then(|callback| {
                callback
                    .map(|callback| {
                        serde_json::to_string(&callback)
                            .map_err(|error| crate::Error::Host(error.to_string()))
                    })
                    .transpose()
            })
            .map_err(js)
    }

    #[wasm_bindgen(js_name = mixAudio)]
    pub fn mix_audio(
        &mut self,
        frames: u32,
        rate: f64,
        now_ms: f64,
    ) -> std::result::Result<BrowserStereoBlock, JsError> {
        self.kernel
            .mix_audio(frames, rate, now_ms)
            .map(BrowserStereoBlock)
            .map_err(js)
    }

    #[wasm_bindgen(js_name = scheduleCall)]
    pub fn schedule_call(
        &mut self,
        id: u32,
        args: &[u32],
        now: f64,
    ) -> std::result::Result<String, JsError> {
        serde_json::to_string(
            &self
                .kernel
                .schedule_call(&mut self.process, id, args, now)
                .map_err(js)?,
        )
        .map_err(|e| JsError::new(&e.to_string()))
    }
    /// Preserve structured wait/exit outcomes, but return immediate values
    /// directly so uncontended locks need neither a slice copy nor JSON.
    #[wasm_bindgen(js_name = scheduleCallSmall)]
    #[allow(clippy::too_many_arguments)] // Scalar ABI avoids the temporary argument slice.
    pub fn schedule_call_small(
        &mut self,
        id: u32,
        count: u32,
        first: u32,
        second: u32,
        third: u32,
        fourth: u32,
        now: f64,
    ) -> std::result::Result<JsValue, JsError> {
        let args = [first, second, third, fourth];
        let args = args
            .get(..count as usize)
            .ok_or_else(|| JsError::new("scalar scheduled call supports at most four arguments"))?;
        let outcome = self
            .kernel
            .schedule_call(&mut self.process, id, args, now)
            .map_err(js)?;
        if let crate::kernel::ScheduleOutcome::Return { value } = outcome {
            Ok(JsValue::from(value))
        } else {
            serde_json::to_string(&outcome)
                .map(|json| JsValue::from_str(&json))
                .map_err(|error| JsError::new(&error.to_string()))
        }
    }
    #[wasm_bindgen(js_name = advanceTime)]
    pub fn advance_time(&mut self, now: f64) -> std::result::Result<(), JsError> {
        self.kernel.advance_time(now).map_err(js)
    }
    #[wasm_bindgen(js_name = takeReadyWaits)]
    pub fn take_ready_waits(&mut self) -> String {
        serde_json::to_string(&self.kernel.take_ready_waits()).unwrap()
    }
    #[wasm_bindgen(js_name = takeRunnableThreads)]
    pub fn take_runnable_threads(&mut self) -> String {
        serde_json::to_string(&self.kernel.take_runnable_threads()).unwrap()
    }
    #[wasm_bindgen(js_name = finishThread)]
    pub fn finish_thread(&mut self, id: u32, code: u32) -> std::result::Result<(), JsError> {
        self.kernel.finish_thread(id, code).map_err(js)
    }
    #[wasm_bindgen(js_name = cancelWait)]
    pub fn cancel_wait(&mut self, token: u32) -> bool {
        self.kernel.cancel_wait(token)
    }
    #[wasm_bindgen(js_name = beginException)]
    pub fn begin_exception(
        &mut self,
        id: u32,
        args: &[u32],
        registers: &[u32],
    ) -> std::result::Result<String, JsError> {
        let outcome = self
            .kernel
            .begin_exception(&mut self.process, id, args, registers)
            .map_err(js)?;
        serde_json::to_string(&outcome).map_err(|e| JsError::new(&e.to_string()))
    }
    #[wasm_bindgen(js_name = resumeException)]
    pub fn resume_exception(
        &mut self,
        token: u32,
        registers: &[u32],
    ) -> std::result::Result<String, JsError> {
        let outcome = self
            .kernel
            .resume_exception(&mut self.process, token, registers)
            .map_err(js)?;
        serde_json::to_string(&outcome).map_err(|e| JsError::new(&e.to_string()))
    }
    #[wasm_bindgen(js_name = cancelException)]
    pub fn cancel_exception(&mut self, token: u32) -> bool {
        self.kernel.cancel_exception(token)
    }
    #[wasm_bindgen(js_name = takeFrame)]
    pub fn take_frame(&mut self) -> Option<BrowserFrame> {
        self.kernel.take_frame().map(BrowserFrame)
    }
    #[wasm_bindgen(js_name = mouseInput)]
    pub fn mouse_input(
        &mut self,
        x: i32,
        y: i32,
        buttons: u32,
    ) -> std::result::Result<(), JsError> {
        self.kernel.mouse_input(x, y, buttons).map_err(js)
    }
    #[wasm_bindgen(js_name = keyInput)]
    pub fn key_input(
        &mut self,
        key: u32,
        down: bool,
        repeat: bool,
    ) -> std::result::Result<(), JsError> {
        self.kernel.key_input(key, down, repeat).map_err(js)
    }

    #[wasm_bindgen(constructor)]
    pub fn new(
        memory: Memory,
        allocation_start: u32,
        limit: f64,
    ) -> std::result::Result<Self, JsError> {
        if !limit.is_finite() || limit.fract() != 0.0 || !(1.0..=4294967296.0).contains(&limit) {
            return Err(js(Error::InvalidRange));
        }
        Process::new(
            BrowserMemory::new(memory).map_err(js)?,
            allocation_start,
            limit as u64,
        )
        .map(|process| Self {
            process,
            kernel: crate::kernel::Kernel::default(),
            sync_call_error: None,
        })
        .map_err(js)
    }

    /// Resolve a supported Windows import to a JSON service descriptor.
    pub fn resolve(&self, dll: &str, name: &str) -> Option<String> {
        crate::kernel::resolve(dll, name).map(|api| {
            serde_json::json!({ "id": api.id, "arity": api.arity,
                "dll": api.dll, "stackArgs": api.stack_args, "callback": api.callback, "variadic": api.variadic, "clock": api.clock, "wallClock": api.wall_clock, "media": api.media, "storage": api.storage, "schedule": api.schedule, "wake": api.wake })
            .to_string()
        })
    }

    #[wasm_bindgen(js_name = addFile)]
    pub fn add_file(&mut self, path: &str, bytes: &[u8]) -> std::result::Result<(), JsError> {
        self.kernel.add_file(path, bytes).map_err(js)
    }

    #[wasm_bindgen(js_name = addFileReader)]
    pub fn add_file_reader(
        &mut self,
        path: &str,
        size: f64,
        reader: Function,
    ) -> std::result::Result<(), JsError> {
        if !size.is_finite() || size < 0.0 || size.fract() != 0.0 || size > f64::from(u32::MAX) {
            return Err(js(Error::InvalidRange));
        }
        self.kernel
            .add_file_reader(
                path,
                size as u64,
                std::rc::Rc::new(move |offset, length| {
                    let result = reader
                        .call2(
                            &JsValue::UNDEFINED,
                            &JsValue::from_f64(offset as f64),
                            &JsValue::from_f64(length as f64),
                        )
                        .map_err(host)?;
                    let bytes = result.dyn_into::<Uint8Array>().map_err(host)?;
                    Ok(bytes.to_vec())
                }),
            )
            .map_err(js)
    }

    pub fn imports(&self) -> String {
        crate::kernel::manifest_json()
    }

    #[wasm_bindgen(js_name = importRegistry)]
    pub fn import_registry(&mut self, json: &str) -> std::result::Result<(), JsError> {
        self.kernel.import_registry(json).map_err(js)
    }

    #[wasm_bindgen(js_name = setCommandLine)]
    pub fn set_command_line(&mut self, address: u32) -> std::result::Result<(), JsError> {
        self.kernel
            .set_command_line(&self.process, address)
            .map_err(js)
    }

    #[wasm_bindgen(js_name = takeRegistryChanges)]
    pub fn take_registry_changes(&mut self) -> std::result::Result<Option<String>, JsError> {
        self.kernel.take_registry_changes().map_err(js)
    }

    #[wasm_bindgen(js_name = restoreJournal)]
    pub fn restore_journal(&mut self, bytes: &[u8]) -> std::result::Result<usize, JsError> {
        self.kernel.restore_journal(bytes).map_err(js)
    }

    #[wasm_bindgen(js_name = takeFileChanges)]
    pub fn take_file_changes(&mut self) -> Vec<u8> {
        self.kernel.take_file_changes()
    }

    #[wasm_bindgen(js_name = installModules)]
    pub fn install_modules(&mut self, configuration: &str) -> std::result::Result<(), JsError> {
        self.kernel.install_modules(configuration).map_err(js)
    }

    #[wasm_bindgen(js_name = exportAddress)]
    pub fn export_address(&self, dll: &str, symbol: JsValue) -> u32 {
        if let Some(name) = symbol.as_string() {
            if let Some(ordinal) = name
                .strip_prefix('#')
                .and_then(|number| number.parse::<u32>().ok())
            {
                return self
                    .kernel
                    .export_address(dll, None, Some(ordinal))
                    .unwrap_or(0);
            }
            return self
                .kernel
                .export_address(dll, Some(&name), None)
                .unwrap_or(0);
        }
        symbol
            .as_f64()
            .filter(|number| {
                number.is_finite() && number.fract() == 0.0 && (0.0..=65535.0).contains(number)
            })
            .and_then(|ordinal| self.kernel.export_address(dll, None, Some(ordinal as u32)))
            .unwrap_or(0)
    }

    pub fn call(
        &mut self,
        id: u32,
        args: &[u32],
        now_ms: Option<f64>,
        wall_ms: Option<f64>,
    ) -> std::result::Result<u32, JsError> {
        let api = crate::kernel::manifest()
            .get(id as usize)
            .ok_or_else(|| JsError::new("unknown kernel API id"))?;
        if api.wall_clock.is_some() {
            self.kernel
                .set_wall_time(
                    wall_ms
                        .ok_or_else(|| JsError::new("wall-clock API requires a fresh sample"))?,
                )
                .map_err(js)?;
        }
        if api.clock {
            self.kernel
                .set_monotonic_time(
                    now_ms.ok_or_else(|| {
                        JsError::new("clock API requires a fresh monotonic sample")
                    })?,
                )
                .map_err(js)?;
        } else if now_ms.is_some() {
            return Err(JsError::new("monotonic sample provided to a non-clock API"));
        }
        self.kernel.call(&mut self.process, id, args).map_err(js)
    }

    /// Common synchronous calls cross the JS/Wasm boundary with scalar
    /// arguments, avoiding a temporary allocation for wasm-bindgen's slice.
    /// Keep the same arity, clock, memory and API validation as `call`.
    #[wasm_bindgen(js_name = callSmall)]
    #[allow(clippy::too_many_arguments)] // Scalar ABI deliberately avoids a slice allocation.
    pub fn call_small(
        &mut self,
        id: u32,
        count: u32,
        first: u32,
        second: u32,
        third: u32,
        fourth: u32,
        now_ms: Option<f64>,
        wall_ms: Option<f64>,
    ) -> std::result::Result<u32, JsError> {
        let args = [first, second, third, fourth];
        let args = args
            .get(..count as usize)
            .ok_or_else(|| JsError::new("scalar call supports at most four arguments"))?;
        self.call(id, args, now_ms, wall_ms)
    }

    /// Select the calling thread and invoke the scalar API in one boundary crossing.
    /// Selection precedes validation, just as `setThread` followed by `callSmall` does.
    #[wasm_bindgen(js_name = callSmallForThread)]
    #[allow(clippy::too_many_arguments)] // Scalar ABI deliberately avoids a slice allocation.
    pub fn call_small_for_thread(
        &mut self,
        thread_id: u32,
        id: u32,
        count: u32,
        first: u32,
        second: u32,
        third: u32,
        fourth: u32,
        now_ms: Option<f64>,
        wall_ms: Option<f64>,
    ) -> std::result::Result<u32, JsError> {
        self.set_thread(thread_id)?;
        self.call_small(id, count, first, second, third, fourth, now_ms, wall_ms)
    }

    /// A DWORD fits exactly in f64, leaving -1 for transport failure. Returning
    /// one scalar avoids allocating a JS array for the Result multi-value ABI.
    /// The synchronous caller consumes an error before another guest can run.
    #[wasm_bindgen(js_name = callSmallForThreadStatus)]
    #[allow(clippy::too_many_arguments)]
    pub fn call_small_for_thread_status(
        &mut self,
        thread_id: u32,
        id: u32,
        count: u32,
        first: u32,
        second: u32,
        third: u32,
        fourth: u32,
        now_ms: Option<f64>,
        wall_ms: Option<f64>,
    ) -> f64 {
        self.sync_call_error = None;
        match self.call_small_for_thread(
            thread_id, id, count, first, second, third, fourth, now_ms, wall_ms,
        ) {
            Ok(value) => f64::from(value),
            Err(error) => {
                self.sync_call_error = Some(error.into());
                -1.0
            }
        }
    }

    #[wasm_bindgen(js_name = takeSyncCallError)]
    pub fn take_sync_call_error(&mut self) -> JsValue {
        self.sync_call_error.take().unwrap_or(JsValue::UNDEFINED)
    }

    #[wasm_bindgen(js_name = beginCall)]
    pub fn begin_call(&mut self, id: u32, args: &[u32]) -> std::result::Result<String, JsError> {
        let outcome = self
            .kernel
            .begin_call(&mut self.process, id, args)
            .map_err(js)?;
        serde_json::to_string(&outcome).map_err(|error| JsError::new(&error.to_string()))
    }

    #[wasm_bindgen(js_name = resumeCall)]
    pub fn resume_call(&mut self, token: u32, result: u32) -> std::result::Result<String, JsError> {
        let outcome = self
            .kernel
            .resume_call(&mut self.process, token, result)
            .map_err(js)?;
        serde_json::to_string(&outcome).map_err(|error| JsError::new(&error.to_string()))
    }

    #[wasm_bindgen(js_name = cancelCall)]
    pub fn cancel_call(&mut self, token: u32) -> bool {
        self.kernel.cancel_call(token)
    }

    #[wasm_bindgen(js_name = cpuHelper)]
    pub fn cpu_helper(
        &mut self,
        id: u32,
        registers: &[i32],
        monotonic_ms: f64,
    ) -> std::result::Result<Vec<i32>, JsError> {
        let registers: [i32; 16] = registers
            .try_into()
            .map_err(|_| JsError::new("CPU helper requires exactly 16 registers"))?;
        self.process
            .cpu_helper(id, registers, monotonic_ms)
            .map(|registers| registers.to_vec())
            .map_err(js)
    }

    #[wasm_bindgen(js_name = setThread)]
    pub fn set_thread(&mut self, id: u32) -> std::result::Result<(), JsError> {
        self.kernel.set_thread(id).map_err(js)
    }

    pub fn reserve(&mut self, address: u32, size: u32) -> std::result::Result<(), JsError> {
        self.process.reserve(address, size).map_err(js)
    }
    pub fn adopt(
        &mut self,
        address: u32,
        size: u32,
        writable: bool,
    ) -> std::result::Result<(), JsError> {
        self.process.adopt(address, size, writable).map_err(js)
    }
    pub fn map(
        &mut self,
        address: u32,
        bytes: &[u8],
        writable: bool,
    ) -> std::result::Result<(), JsError> {
        self.process.map(address, bytes, writable).map_err(js)
    }
    pub fn write(&mut self, address: u32, bytes: &[u8]) -> std::result::Result<(), JsError> {
        self.process.write(address, bytes).map_err(js)
    }
    pub fn read(&self, address: u32, size: u32) -> std::result::Result<Vec<u8>, JsError> {
        let mut bytes = Vec::new();
        bytes
            .try_reserve_exact(size as usize)
            .map_err(|_| js(Error::OutOfMemory))?;
        bytes.resize(size as usize, 0);
        self.process.read(address, &mut bytes).map_err(js)?;
        Ok(bytes)
    }
    #[wasm_bindgen(js_name = heapAlloc)]
    pub fn heap_alloc(&mut self, size: u32) -> std::result::Result<u32, JsError> {
        self.process.heap_alloc(size).map_err(js)
    }
    #[wasm_bindgen(js_name = heapFree)]
    pub fn heap_free(&mut self, address: u32) -> std::result::Result<(), JsError> {
        self.process.heap_free(address).map_err(js)
    }
    #[wasm_bindgen(js_name = heapSize)]
    pub fn heap_size(&self, address: u32) -> std::result::Result<u32, JsError> {
        self.process.heap_size(address).map_err(js)
    }
    #[wasm_bindgen(js_name = heapRealloc)]
    pub fn heap_realloc(&mut self, address: u32, size: u32) -> std::result::Result<u32, JsError> {
        self.process.heap_realloc(address, size).map_err(js)
    }
    #[wasm_bindgen(js_name = virtualAlloc)]
    pub fn virtual_alloc(&mut self, address: u32, size: u32) -> std::result::Result<u32, JsError> {
        self.process.virtual_alloc(address, size).map_err(js)
    }
    #[wasm_bindgen(js_name = virtualFree)]
    pub fn virtual_free(&mut self, address: u32) -> std::result::Result<(), JsError> {
        self.process.virtual_free(address).map_err(js)
    }
}

/// A completed RGBA frame copied out once by the browser presentation host.
#[wasm_bindgen(js_name = Frame)]
pub struct BrowserFrame(crate::Frame);
#[wasm_bindgen(js_class = Frame)]
impl BrowserFrame {
    #[wasm_bindgen(getter)]
    pub fn width(&self) -> u32 {
        self.0.width
    }
    #[wasm_bindgen(getter)]
    pub fn height(&self) -> u32 {
        self.0.height
    }
    pub fn pixels(&self) -> Uint8Array {
        Uint8Array::from(self.0.pixels.as_slice())
    }
}

/// Owned PCM block: only stereo sample arrays cross the browser boundary.
#[wasm_bindgen(js_name = StereoBlock)]
pub struct BrowserStereoBlock(crate::audio::StereoBlock);
#[wasm_bindgen(js_class = StereoBlock)]
impl BrowserStereoBlock {
    pub fn left(&self) -> js_sys::Float32Array {
        js_sys::Float32Array::from(self.0.left.as_slice())
    }
    pub fn right(&self) -> js_sys::Float32Array {
        js_sys::Float32Array::from(self.0.right.as_slice())
    }
}

#[wasm_bindgen(js_class = Process)]
impl BrowserProcess {
    #[wasm_bindgen(js_name = takePackedFrame)]
    pub fn take_packed_frame(&mut self) -> Option<BrowserPackedFrame> {
        self.kernel.take_packed_frame().map(BrowserPackedFrame)
    }
}

/// Retains the exact completed surface until its packed bytes are copied to the host.
#[wasm_bindgen(js_name = PackedFrame)]
pub struct BrowserPackedFrame(crate::PackedFrame);
#[wasm_bindgen(js_class = PackedFrame)]
impl BrowserPackedFrame {
    #[wasm_bindgen(getter)]
    pub fn width(&self) -> u32 {
        self.0.width
    }
    #[wasm_bindgen(getter)]
    pub fn height(&self) -> u32 {
        self.0.height
    }
    #[wasm_bindgen(getter)]
    pub fn pitch(&self) -> u32 {
        self.0.pitch
    }
    #[wasm_bindgen(getter)]
    pub fn format(&self) -> u32 {
        self.0.format
    }
    pub fn pixels(&self) -> Uint8Array {
        Uint8Array::from(self.0.pixels.as_slice())
    }
}

#[wasm_bindgen(js_class = Process)]
impl BrowserProcess {
    #[wasm_bindgen(js_name = hasReadyWaits)]
    pub fn has_ready_waits(&self) -> bool {
        self.kernel.has_ready_waits()
    }
}
