//! First-thread Windows startup services. Unknown imports and unsupported modes fail explicitly.
mod callbacks;
mod com;
mod compatibility;
mod crt;
mod directshow;
mod events;
mod exceptions;
pub use exceptions::ExceptionAction;
mod formatting;
mod images;
mod local_memory;
mod locale;
mod manifest;
mod math;
mod media;
mod memory;
mod modules;
mod resources;
mod scheduling;
mod system;
pub use scheduling::{ExecutionDescriptor, ReadyWait, ScheduleOutcome};
mod startup;
mod synchronization;
mod text;
mod timing;
mod version;
mod wall_time;
use crate::{Error, GuestMemory, Process, Result};
use manifest::Call;
pub use manifest::{manifest, manifest_json, resolve, Api};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Default)]
struct Thread {
    apartment: Option<com::Apartment>,
    priority: i32,
    error: u32,
    tls: BTreeMap<u32, u32>,
}
#[derive(Default)]
struct CriticalSection {
    owner: Option<u32>,
    depth: u32,
    queue: std::collections::VecDeque<(u32, u32)>,
}

pub struct Kernel {
    exceptions: exceptions::Exceptions,
    wall_time: Option<i64>,
    executions: scheduling::Executions,
    media: crate::media::Media,
    directshow: directshow::DirectShow,
    audio: crate::audio::Audio,
    images: images::Images,
    events: events::Events,
    clock: timing::Clock,
    graphics: crate::graphics::Graphics,
    user: crate::user::User,
    callbacks: crate::callbacks::Callbacks,
    callback_owners: BTreeMap<u32, u32>,
    registry: crate::registry::Registry,
    files: crate::files::Files,
    modules: crate::modules::ModuleCatalog,
    thread_id: u32,
    threads: BTreeMap<u32, Thread>,
    tls_slots: BTreeSet<u32>,
    heaps: BTreeSet<u32>,
    locks: BTreeMap<u32, CriticalSection>,
    environment: BTreeSet<u32>,
    command_line: Option<u32>,
    locale_name: Option<u32>,
    error_mode: u32,
    unhandled_exception_filter: u32,
}
impl Default for Kernel {
    fn default() -> Self {
        let display = crate::display::Display::default();
        Self {
            exceptions: exceptions::Exceptions::default(),
            wall_time: None,
            executions: scheduling::Executions::default(),
            media: crate::media::Media::default(),
            directshow: directshow::DirectShow::default(),
            audio: crate::audio::Audio::default(),
            images: images::Images::default(),
            events: events::Events::default(),
            clock: timing::Clock::default(),
            graphics: crate::graphics::Graphics::with_display(display.clone()),
            user: crate::user::User::with_display(display),
            callbacks: crate::callbacks::Callbacks::default(),
            callback_owners: BTreeMap::new(),
            registry: crate::registry::Registry::default(),
            files: crate::files::Files::default(),
            modules: crate::modules::ModuleCatalog::default(),
            thread_id: 1,
            threads: BTreeMap::new(),
            tls_slots: BTreeSet::new(),
            heaps: BTreeSet::from([1]),
            locks: BTreeMap::new(),
            environment: BTreeSet::new(),
            command_line: None,
            locale_name: None,
            error_mode: 0,
            unhandled_exception_filter: 0,
        }
    }
}
impl Kernel {
    /// Use the loader's command-line buffer for GetCommandLineA as well as the
    /// translated entry point. Configure this before starting guest execution.
    pub fn set_command_line<M: GuestMemory>(
        &mut self,
        process: &Process<M>,
        address: u32,
    ) -> Result<()> {
        process.read_c_string(address, 4096)?;
        self.command_line = Some(address);
        Ok(())
    }

    pub fn mix_audio(
        &mut self,
        frames: u32,
        rate: f64,
        now_ms: f64,
    ) -> Result<crate::audio::StereoBlock> {
        let mut block = self.audio.mix(frames, rate, now_ms)?;
        self.directshow
            .mix(&mut block, rate as u32, &mut self.user)?;
        Ok(block)
    }

    pub fn take_frame(&mut self) -> Option<crate::Frame> {
        self.graphics.take_frame()
    }
    pub fn take_packed_frame(&mut self) -> Option<crate::PackedFrame> {
        self.graphics.take_packed_frame()
    }
    pub fn mouse_input(&mut self, x: i32, y: i32, buttons: u32) -> Result<()> {
        self.user.mouse_input(x, y, buttons)
    }
    pub fn key_input(&mut self, key: u32, down: bool, repeat: bool) -> Result<()> {
        self.user.key_input(key, down, repeat)
    }

    pub fn add_file(&mut self, path: &str, bytes: &[u8]) -> Result<()> {
        self.files.add_file(path, bytes)
    }

    /// Register immutable contents without reading them. The reader must return
    /// exactly the requested range and must not reenter this kernel.
    pub fn add_file_reader(
        &mut self,
        path: &str,
        size: u64,
        reader: std::rc::Rc<crate::files::FileReader>,
    ) -> Result<()> {
        self.files.add_file_source(path, size, reader)
    }

    pub fn import_registry(&mut self, json: &str) -> Result<()> {
        self.registry.import_snapshot(json)
    }

    pub fn take_registry_changes(&mut self) -> Result<Option<String>> {
        self.registry.take_changes()
    }

    pub fn restore_journal(&mut self, bytes: &[u8]) -> Result<usize> {
        self.files.restore_journal(bytes)
    }

    pub fn take_file_changes(&mut self) -> Vec<u8> {
        self.files.take_file_changes()
    }

    pub fn install_modules(&mut self, json: &str) -> Result<()> {
        self.modules = crate::modules::ModuleCatalog::from_json(json)?;
        Ok(())
    }

    pub fn set_thread(&mut self, id: u32) -> Result<()> {
        if id == 0 {
            return Err(Error::InvalidRange);
        }
        self.thread_id = id;
        Ok(())
    }
    fn thread(&mut self) -> &mut Thread {
        self.threads.entry(self.thread_id).or_default()
    }
    fn fail(&mut self, error: u32) -> u32 {
        self.thread().error = error;
        0
    }
    fn heap(&self, handle: u32) -> Result<()> {
        if self.heaps.contains(&handle) {
            Ok(())
        } else {
            Err(Error::Host(format!("invalid heap handle {handle:#x}")))
        }
    }

    pub fn call<M: GuestMemory>(
        &mut self,
        process: &mut Process<M>,
        id: u32,
        args: &[u32],
    ) -> Result<u32> {
        let api = manifest()
            .get(id as usize)
            .ok_or_else(|| Error::Host(format!("unknown kernel API id {id}")))?;
        let argument_count = api.arity as usize + usize::from(api.variadic);
        if args.len() != argument_count {
            return Err(Error::Host(format!(
                "{} expects {} arguments, got {}",
                api.name,
                argument_count,
                args.len()
            )));
        }
        if api.callback {
            return Err(Error::Host(format!("{} requires beginCall", api.name)));
        }
        if api.dll == "quartz.dll" {
            return self.graph_call(process, api.name, args);
        }
        if api.dll == "dsound.dll" {
            let now = if api.clock {
                self.take_monotonic_time()?
            } else {
                0.0
            };
            return self.audio.call(process, &self.modules, api.name, args, now);
        }
        if api.dll == "ddraw.dll" {
            if matches!(api.name, "Surface_GetDC" | "Surface_ReleaseDC") {
                return self.graphics.surface_dc(
                    process,
                    &mut self.user,
                    api.name,
                    args,
                    self.thread_id,
                );
            }
            return self.graphics.call(process, &self.modules, api.name, args);
        }
        match api.call {
            Call::CreateDIBSection
            | Call::CreateCompatibleDC
            | Call::DeleteDC
            | Call::SelectObject
            | Call::CreateFontA
            | Call::CreateSolidBrush
            | Call::UnrealizeObject
            | Call::GetSystemPaletteEntries
            | Call::DPtoLP => self
                .user
                .gdi_compatibility(process, api.name, args, self.thread_id),
            Call::GetLocalTime
            | Call::GetSystemTime
            | Call::GetSystemTimeAsFileTime
            | Call::FileTimeToLocalFileTime
            | Call::FileTimeToSystemTime => self.wall_time_call(process, api.call, args),
            Call::CompareStringA
            | Call::CompareStringW
            | Call::LCMapStringA
            | Call::LstrcmpiA
            | Call::LstrcpynA
            | Call::LstrlenA => self.legacy_text(process, api.call, args),
            Call::CoCreateInstance => self.create_graph(process, args),
            Call::CreateMutexA | Call::CreateEventA => {
                self.create_named_object(process, api.call, args)
            }
            Call::GlobalMemoryStatus
            | Call::GetEnvironmentVariableA
            | Call::FormatMessageA
            | Call::ShellExecuteExA => self.system_call(process, api.call, args),
            Call::ShellExecuteA
            | Call::DirectInputCreateA
            | Call::ImmGetContext
            | Call::ImmReleaseContext
            | Call::ImmGetDefaultIMEWnd
            | Call::ImmGetCompositionStringA
            | Call::ImmGetCompositionStringW
            | Call::ImmGetCandidateListA
            | Call::HeapDestroy
            | Call::SetStdHandle
            | Call::QueueUserAPC
            | Call::SetFileAttributesA
            | Call::RemoveDirectoryA
            | Call::TerminateProcess
            | Call::GetProcessWorkingSetSize
            | Call::SetProcessWorkingSetSize
            | Call::SetConsoleCtrlHandler
            | Call::UnhandledExceptionFilter
            | Call::GetProcessHeaps
            | Call::IsBadReadPtr
            | Call::IsBadWritePtr
            | Call::IsBadCodePtr
            | Call::CreateProcessA
            | Call::OpenProcess
            | Call::Lclose
            | Call::ExitProcess
            | Call::FatalAppExitA
            | Call::OutputDebugStringA
            | Call::GetDriveTypeA
            | Call::SetEnvironmentVariableA
            | Call::SetEnvironmentVariableW
            | Call::GetExitCodeProcess
            | Call::GetSystemDirectoryA
            | Call::GetSystemInfo
            | Call::VirtualQuery
            | Call::OpenFile => self.compatibility(process, api.call, args),
            Call::CreateThread
            | Call::ResumeThread
            | Call::SuspendThread
            | Call::TerminateThread
            | Call::GetExitCodeThread
            | Call::DuplicateHandle
            | Call::CreatePipe => self.thread_call(process, api.call, args),
            Call::WaveOutGetNumDevs
            | Call::WaveOutGetDevCapsA
            | Call::WaveOutOpen
            | Call::WaveOutClose
            | Call::WaveOutPrepareHeader
            | Call::WaveOutUnprepareHeader
            | Call::WaveOutWrite
            | Call::WaveOutPause
            | Call::WaveOutRestart
            | Call::WaveOutReset
            | Call::WaveOutGetPosition
            | Call::WaveOutSetVolume
            | Call::WaveOutGetVolume
            | Call::MidiOutGetNumDevs
            | Call::MidiOutGetDevCapsA
            | Call::MidiOutOpen
            | Call::MidiOutClose
            | Call::MidiOutShortMsg
            | Call::MidiStreamOpen
            | Call::MidiStreamClose
            | Call::MidiStreamOut
            | Call::MidiStreamPause
            | Call::MidiStreamRestart
            | Call::MidiStreamStop
            | Call::MidiStreamSetPosition
            | Call::TimeBeginPeriod
            | Call::TimeGetDevCaps
            | Call::TimeEndPeriod
            | Call::TimeSetEvent
            | Call::TimeKillEvent
            | Call::MmioOpenA
            | Call::MmioRead
            | Call::MmioSeek
            | Call::MmioSetBuffer
            | Call::MmioClose => self.media_call(process, api.call, api.name, args),
            Call::IsValidLocale => Ok(self.valid_locale(args)),
            Call::GetLocaleInfoA | Call::GetLocaleInfoW => {
                self.locale_info(process, args, api.call == Call::GetLocaleInfoW)
            }
            Call::GetStockObject
            | Call::GetObjectA
            | Call::DeleteObject
            | Call::CreatePalette
            | Call::GetPaletteEntries => {
                let result = self.user.object_call(process, api.name, args);
                if let Some(error) = self.user.take_error() {
                    self.thread().error = error;
                }
                result
            }
            Call::LoadIconA | Call::LoadCursorA => self.load_image(process, api.call, args),
            Call::GetDC
            | Call::ReleaseDC
            | Call::SelectPalette
            | Call::RealizePalette
            | Call::GetDeviceCaps
            | Call::SetMapMode
            | Call::GetMapMode
            | Call::SetBkMode
            | Call::GetBkMode
            | Call::SetBkColor
            | Call::GetBkColor
            | Call::SetTextColor
            | Call::GetTextColor => {
                let result = self.user.dc_call(api.name, args, self.thread_id);
                if let Some(error) = self.user.take_error() {
                    self.thread().error = error;
                }
                result
            }
            Call::CoInitialize | Call::CoInitializeEx | Call::CoUninitialize => {
                self.com_initialize(api.call, args)
            }
            Call::CloseHandle if (0x6000_0000..0x7000_0000).contains(&args[0]) => {
                self.event_call(api.call, args)
            }
            Call::ReleaseMutex
            | Call::SetEvent
            | Call::PulseEvent
            | Call::ResetEvent
            | Call::WaitForSingleObject => self.event_call(api.call, args),
            Call::MulDiv => {
                Ok(math::mul_div(args[0] as i32, args[1] as i32, args[2] as i32) as u32)
            }
            Call::InterlockedIncrement
            | Call::InterlockedDecrement
            | Call::InterlockedExchange
            | Call::InterlockedExchangeAdd
            | Call::InterlockedCompareExchange => self.interlocked(process, api.call, args),
            Call::GetTimeZoneInformation => self.time_zone(process, args[0]),
            Call::QueryPerformanceFrequency
            | Call::QueryPerformanceCounter
            | Call::GetTickCount
            | Call::TimeGetTime => self.time_call(process, api.call, args),
            // Registration is process-wide. Exception delivery remains unsupported.
            Call::SetUnhandledExceptionFilter => Ok(std::mem::replace(
                &mut self.unhandled_exception_filter,
                args[0],
            )),
            Call::LCMapStringW => self.map_string(process, args),
            Call::WsprintfA | Call::WvsprintfA => self.wsprintf(process, args),
            Call::LocalAlloc
            | Call::LocalFree
            | Call::LocalReAlloc
            | Call::LocalSize
            | Call::LocalLock
            | Call::LocalUnlock
            | Call::GlobalAlloc
            | Call::GlobalFree
            | Call::GlobalReAlloc
            | Call::GlobalSize
            | Call::GlobalLock
            | Call::GlobalUnlock => self.local_memory(process, api.call, args),
            Call::LoadCursorFromFileA
            | Call::DestroyCursor
            | Call::MoveWindow
            | Call::MsgWaitForMultipleObjects
            | Call::BeginPaint
            | Call::EndPaint
            | Call::SetForegroundWindow
            | Call::GetParent
            | Call::FillRect
            | Call::CreateMenu
            | Call::CreatePopupMenu
            | Call::AppendMenuA
            | Call::DestroyMenu
            | Call::TrackPopupMenu
            | Call::GetMenu
            | Call::MessageBoxA
            | Call::MessageBoxW
            | Call::OpenClipboard
            | Call::CloseClipboard
            | Call::EmptyClipboard
            | Call::SetClipboardData
            | Call::GetClipboardData
            | Call::SystemParametersInfoA
            | Call::ChangeDisplaySettingsA
            | Call::SetWindowTextA
            | Call::GetKeyboardState
            | Call::ShowCursor
            | Call::SetCursor
            | Call::SetCursorPos
            | Call::GetCapture
            | Call::InvalidateRect
            | Call::IsWindowVisible
            | Call::DrawTextA
            | Call::UpdateWindow
            | Call::SetRect
            | Call::OffsetRect
            | Call::InflateRect
            | Call::IntersectRect
            | Call::UnionRect
            | Call::IsRectEmpty
            | Call::PtInRect
            | Call::FindWindowA
            | Call::FindWindowExA
            | Call::GetCursorPos
            | Call::GetKeyState
            | Call::GetAsyncKeyState
            | Call::GetFocus
            | Call::SetFocus
            | Call::RegisterClassA
            | Call::RegisterClassExA
            | Call::GetWindowLongA
            | Call::SetWindowLongA
            | Call::GetClassInfoA
            | Call::GetClassInfoExA
            | Call::DefWindowProcA
            | Call::ShowWindow
            | Call::EnumDisplaySettingsA
            | Call::GetSystemMetrics
            | Call::AdjustWindowRect
            | Call::AdjustWindowRectEx
            | Call::GetClientRect
            | Call::GetWindowRect
            | Call::ClientToScreen
            | Call::ScreenToClient
            | Call::PeekMessageA
            | Call::GetMessageA
            | Call::TranslateMessage
            | Call::PostMessageA
            | Call::PostQuitMessage
            | Call::IsWindow => {
                if matches!(api.call, Call::PeekMessageA | Call::GetMessageA) {
                    let time = self.window_time()?;
                    self.user.set_time(time);
                }
                let result = self.user.call(process, api.name, args);
                if let Some(error) = self.user.take_error() {
                    self.thread().error = error;
                }
                result
            }
            Call::GetFileVersionInfoSizeA | Call::GetFileVersionInfoA | Call::VerQueryValueA => {
                self.version(process, api.call, args)
            }
            Call::Initterm => Err(Error::Host("_initterm requires beginCall".into())),
            Call::RegOpenKeyExA
            | Call::RegCreateKeyExA
            | Call::RegCloseKey
            | Call::RegQueryValueExA
            | Call::RegSetValueExA
            | Call::RegDeleteValueA
            | Call::RegDeleteKeyA
            | Call::RegOpenKeyA
            | Call::RegCreateKeyA
            | Call::RegQueryValueA
            | Call::RegSetValueA => self.registry.call(process, api.name, args),
            Call::CreateFileA
            | Call::ReadFile
            | Call::WriteFile
            | Call::SetFilePointer
            | Call::GetFileSize
            | Call::CloseHandle
            | Call::GetFileAttributesA
            | Call::FindFirstFileA
            | Call::FindNextFileA
            | Call::FindClose
            | Call::SetEndOfFile
            | Call::DeleteFileA
            | Call::CopyFileA
            | Call::MoveFileA
            | Call::FlushFileBuffers
            | Call::CreateDirectoryA
            | Call::GetCurrentDirectoryA
            | Call::SetCurrentDirectoryA
            | Call::GetFullPathNameA
            | Call::GetPrivateProfileStringA
            | Call::GetPrivateProfileIntA
            | Call::WritePrivateProfileStringA => {
                let (value, error) = self.files.call(process, api.name, args)?;
                self.thread().error = error;
                Ok(value)
            }
            Call::Malloc
            | Call::Free
            | Call::Realloc
            | Call::Calloc
            | Call::Memcpy
            | Call::Memmove
            | Call::Memset
            | Call::Memcmp
            | Call::Strlen
            | Call::Strcmp
            | Call::Strncmp
            | Call::Strcpy
            | Call::Strncpy
            | Call::Strchr
            | Call::Strrchr
            | Call::Stricmp => crt::call(process, api.call, args),
            Call::GetModuleHandleA
            | Call::GetModuleHandleW
            | Call::GetModuleFileNameA
            | Call::GetModuleFileNameW
            | Call::LoadLibraryA
            | Call::LoadLibraryExA
            | Call::FreeLibrary
            | Call::GetProcAddress => self.module_call(process, api.call, args),
            Call::GetStringTypeA | Call::GetStringTypeW => {
                self.string_type(process, api.call, args)
            }
            Call::GetProcessHeap
            | Call::HeapCreate
            | Call::HeapAlloc
            | Call::HeapFree
            | Call::HeapSize
            | Call::HeapReAlloc
            | Call::VirtualAlloc
            | Call::VirtualFree => self.memory(process, api.call, args),
            Call::InitializeCriticalSection
            | Call::DeleteCriticalSection
            | Call::EnterCriticalSection
            | Call::TryEnterCriticalSection
            | Call::LeaveCriticalSection
            | Call::TlsAlloc
            | Call::TlsFree
            | Call::TlsGetValue
            | Call::TlsSetValue
            | Call::SetThreadPriority
            | Call::GetThreadPriority
            | Call::GetCurrentThreadId
            | Call::GetLastError
            | Call::SetLastError => self.synchronization(process, api.call, args),
            Call::GetVersion
            | Call::GetCurrentProcessId
            | Call::GetCurrentProcess
            | Call::GetCurrentThread
            | Call::GetStartupInfoA
            | Call::GetCommandLineA
            | Call::GetEnvironmentStringsW
            | Call::GetEnvironmentStrings
            | Call::GetEnvironmentStringsA
            | Call::FreeEnvironmentStringsW
            | Call::FreeEnvironmentStringsA
            | Call::GetStdHandle
            | Call::GetFileType
            | Call::SetHandleCount
            | Call::GetACP
            | Call::GetOEMCP
            | Call::GetUserDefaultLCID
            | Call::IsValidCodePage
            | Call::GetCPInfo
            | Call::WideCharToMultiByte
            | Call::MultiByteToWideChar
            | Call::SetErrorMode
            | Call::GetVersionExA => self.startup(process, api.call, args),
            _ => Err(Error::Host(format!(
                "{} domain is not yet connected",
                api.name
            ))),
        }
    }
}

#[cfg(test)]
#[path = "../tests/unit/kernel.rs"]
mod tests;
