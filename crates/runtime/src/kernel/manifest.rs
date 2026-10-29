//! Supported imports have one explicit calling convention and argument count.
#[derive(Clone, Copy, Debug)]
pub struct Api {
    pub id: u32,
    pub callback: bool,
    pub exception: bool,
    /// Host appends the guest address of the first optional DWORD after fixed arguments.
    pub variadic: bool,
    /// This call requires a fresh host monotonic clock sample in milliseconds.
    pub clock: bool,
    pub wall_clock: Option<&'static str>,
    pub schedule: bool,
    pub wake: bool,
    /// Pump the original WinMM function-timer queue before this import.
    pub media: bool,
    /// Flush durable file mutations before returning success to guest code.
    pub storage: bool,
    pub dll: &'static str,
    pub stack_args: u32,
    pub name: &'static str,
    pub arity: u32,
    pub(super) call: Call,
}

macro_rules! api_name {
    ($variant:ident) => {
        stringify!($variant)
    };
    ($variant:ident, $name:literal) => {
        $name
    };
}
macro_rules! stack_args {
    (stdcall, $arity:literal) => {
        $arity
    };
    (cdecl, $arity:literal) => {
        0
    };
}
macro_rules! callback {
    () => {
        false
    };
    (callback) => {
        true
    };
    (variadic) => {
        false
    };
}
macro_rules! variadic {
    () => {
        false
    };
    (callback) => {
        false
    };
    (variadic) => {
        true
    };
}
macro_rules! apis {
    ($($dll:literal, $convention:ident => { $($variant:ident $(=> $name:literal)? ($arity:literal) $([$callback:ident])?),* $(,)? })*) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        #[repr(u32)]
        pub(super) enum Call { $($($variant),*),* }
        pub static APIS: &[Api] = &[$($(Api {
            id: Call::$variant as u32, dll: $dll, name: api_name!($variant $(,$name)?),
            arity: $arity, stack_args: stack_args!($convention, $arity),
            exception: matches!(Call::$variant, Call::RaiseException | Call::RtlUnwind),
            wall_clock: match Call::$variant {Call::GetLocalTime=>Some("local"),Call::GetSystemTime | Call::GetSystemTimeAsFileTime=>Some("utc"),_=>None},
            clock: matches!(Call::$variant, Call::QueryPerformanceCounter | Call::GetTickCount | Call::TimeGetTime | Call::PeekMessageA | Call::GetMessageA | Call::DSBuffer4 | Call::DSBuffer9 | Call::DSBuffer12 | Call::DSBuffer13 | Call::TimeSetEvent),
            media: matches!(Call::$variant, Call::MmioOpenA | Call::MmioRead | Call::MmioSeek | Call::MmioSetBuffer | Call::MmioClose),
            storage: matches!(Call::$variant, Call::CreateFileA | Call::WriteFile | Call::SetEndOfFile | Call::DeleteFileA | Call::CopyFileA | Call::MoveFileA | Call::FlushFileBuffers | Call::WritePrivateProfileStringA),
            schedule: matches!(Call::$variant, Call::WaitForSingleObject | Call::WaitForSingleObjectEx | Call::WaitForMultipleObjects | Call::WaitForMultipleObjectsEx | Call::Sleep | Call::SleepEx | Call::EnterCriticalSection | Call::ExitThread),
            wake: matches!(Call::$variant, Call::CreateThread | Call::ResumeThread | Call::SetEvent | Call::PulseEvent | Call::ReleaseMutex | Call::LeaveCriticalSection | Call::CloseHandle),
            callback: callback!($($callback)?), variadic: variadic!($($callback)?), call: Call::$variant
        }),*),*];
    };
}
apis! {
    "quartz.dll", stdcall => { DGQuery => "DG_Query"(3), DGAddRef => "DG_AddRef"(1), DGRelease => "DG_Release"(1), DGEnum => "DG_Enum"(2), DGRender => "DG_Render"(3), DGRun => "DG_Run"(1), DGPause => "DG_Pause"(1), DGStop => "DG_Stop"(1), DGState => "DG_State"(3), DGVolume => "DG_Volume"(2), DGGetVolume => "DG_GetVolume"(2), DGBalance => "DG_Balance"(2), DGGetBalance => "DG_GetBalance"(2), DGEvent => "DG_Event"(5), DGFreeEvent => "DG_FreeEvent"(4), DGNotify => "DG_Notify"(4), DGFlags => "DG_Flags"(2), DGGetFlags => "DG_GetFlags"(2), DGNext => "DG_Next"(4), DGReset => "DG_Reset"(1), DGSkip => "DG_Skip"(2), DGClone => "DG_Clone"(2), DGUnsupported1 => "DG_Unsupported1"(1), DGUnsupported2 => "DG_Unsupported2"(2), DGUnsupported3 => "DG_Unsupported3"(3), DGUnsupported4 => "DG_Unsupported4"(4), DGUnsupported5 => "DG_Unsupported5"(5), DGUnsupported6 => "DG_Unsupported6"(6), DGUnsupported7 => "DG_Unsupported7"(7), DGUnsupported8 => "DG_Unsupported8"(8), DGUnsupported9 => "DG_Unsupported9"(9) }
    "kernel32.dll", stdcall => { RaiseException(4), RtlUnwind(4),
    GetLocalTime(1), GetSystemTime(1), FileTimeToLocalFileTime(2), FileTimeToSystemTime(2),
    CompareStringA(6), CompareStringW(6), LCMapStringA(6), LstrcmpiA => "lstrcmpiA"(2), LstrcpynA => "lstrcpynA"(3), LstrlenA => "lstrlenA"(1),
    HeapDestroy(1), SetStdHandle(2), QueueUserAPC(3), SetFileAttributesA(2), RemoveDirectoryA(1), TerminateProcess(2), GetProcessWorkingSetSize(3), SetProcessWorkingSetSize(3), SetConsoleCtrlHandler(2), UnhandledExceptionFilter(1), GetProcessHeaps(2), IsBadReadPtr(2), IsBadWritePtr(2), IsBadCodePtr(1), CreateProcessA(10), OpenProcess(3), Lclose => "_lclose"(1), ExitProcess(1), FatalAppExitA(2), OutputDebugStringA(1), GetDriveTypeA(1), SetEnvironmentVariableA(2), SetEnvironmentVariableW(2), GetExitCodeProcess(2), GetSystemDirectoryA(2), GetSystemInfo(1), VirtualQuery(3), OpenFile(3),

    CreateThread(6), ResumeThread(1), SuspendThread(1), TerminateThread(2), ExitThread(1), GetExitCodeThread(2), DuplicateHandle(7), CreatePipe(4), PulseEvent(1), WaitForSingleObjectEx(3), WaitForMultipleObjects(4), WaitForMultipleObjectsEx(5), Sleep(1), SleepEx(2),
    SetThreadPriority(2), GetThreadPriority(1),
    EnumSystemLocalesA(2)[callback], GetLocaleInfoA(4), GetLocaleInfoW(4), IsValidLocale(2),
    CreateEventA(4), SetEvent(1), ResetEvent(1), WaitForSingleObject(2), CreateMutexA(3), ReleaseMutex(1),
    InterlockedIncrement(1), InterlockedDecrement(1), InterlockedExchange(2), InterlockedExchangeAdd(2), InterlockedCompareExchange(3),
    GetTimeZoneInformation(1),
    QueryPerformanceFrequency(1), QueryPerformanceCounter(1), GetTickCount(0),
    LocalAlloc(2), LocalFree(1), LocalReAlloc(3), LocalSize(1), LocalLock(1), LocalUnlock(1),
    GlobalAlloc(2), GlobalFree(1), GlobalReAlloc(3), GlobalSize(1), GlobalLock(1), GlobalUnlock(1),
    MulDiv(3), SetUnhandledExceptionFilter(1),
    GetVersion(0), HeapCreate(3), GetProcessHeap(0), HeapAlloc(3), HeapFree(3), HeapReAlloc(4), HeapSize(3),
    VirtualAlloc(4), VirtualFree(3), InitializeCriticalSection(1), EnterCriticalSection(1),
    LeaveCriticalSection(1), DeleteCriticalSection(1), TryEnterCriticalSection(1),
    TlsAlloc(0), TlsFree(1), TlsGetValue(1), TlsSetValue(2),
    GetCurrentThreadId(0), GetCurrentProcessId(0), GetCurrentProcess(0), GetCurrentThread(0),
    GetStartupInfoA(1), GetCommandLineA(0), GetEnvironmentStringsW(0), GetEnvironmentStrings(0), GetEnvironmentVariableA(3),
    GetSystemTimeAsFileTime(1), GlobalMemoryStatus(1), FormatMessageA(7),
    GetEnvironmentStringsA(0), FreeEnvironmentStringsA(1), FreeEnvironmentStringsW(1),
    GetStdHandle(1), GetFileType(1), SetHandleCount(1), GetACP(0), GetOEMCP(0), GetCPInfo(2),
    WideCharToMultiByte(8), MultiByteToWideChar(6), GetLastError(0), SetLastError(1),
    GetUserDefaultLCID(0), IsValidCodePage(1), SetErrorMode(1), GetVersionExA(1),
    GetModuleHandleA(1), GetModuleHandleW(1), GetModuleFileNameA(3), GetModuleFileNameW(3),
    LoadLibraryA(1), LoadLibraryExA(3), FreeLibrary(1), GetProcAddress(2), GetStringTypeA(5), GetStringTypeW(4), LCMapStringW(6),
    CreateFileA(7), ReadFile(5), WriteFile(5), SetFilePointer(4), GetFileSize(2), CloseHandle(1),
    GetFileAttributesA(1), FindFirstFileA(2), FindNextFileA(2), FindClose(1), SetEndOfFile(1),
    DeleteFileA(1), CopyFileA(3), MoveFileA(2), FlushFileBuffers(1), CreateDirectoryA(2), GetCurrentDirectoryA(2), SetCurrentDirectoryA(1),
    GetFullPathNameA(4), GetPrivateProfileStringA(6), GetPrivateProfileIntA(4), WritePrivateProfileStringA(4)
    }
    "msvcrt.dll", cdecl => {
    Malloc => "malloc"(1), Free => "free"(1), Realloc => "realloc"(2), Calloc => "calloc"(2),
    Memcpy => "memcpy"(3), Memmove => "memmove"(3), Memset => "memset"(3), Memcmp => "memcmp"(3),
    Strlen => "strlen"(1), Strcmp => "strcmp"(2), Strncmp => "strncmp"(3), Strcpy => "strcpy"(2),
    Strncpy => "strncpy"(3), Strchr => "strchr"(2), Strrchr => "strrchr"(2), Stricmp => "_stricmp"(2), Initterm => "_initterm"(2)[callback]
    }
    "advapi32.dll", stdcall => {
    RegOpenKeyExA(5), RegCreateKeyExA(9), RegCloseKey(1), RegQueryValueExA(6), RegSetValueExA(6),
    RegDeleteValueA(2), RegDeleteKeyA(2), RegOpenKeyA(3), RegCreateKeyA(3), RegQueryValueA(4), RegSetValueA(5)
    }
    "user32.dll", stdcall => {
        LoadIconA(2), LoadCursorA(2), LoadCursorFromFileA(1), DestroyCursor(1), SetWindowTextA(2),
        MoveWindow(6), MsgWaitForMultipleObjects(5), BeginPaint(2), EndPaint(2), SetForegroundWindow(1), GetParent(1), FillRect(3),
        CreateMenu(0), CreatePopupMenu(0), AppendMenuA(4), DestroyMenu(1), TrackPopupMenu(6), GetMenu(1),
        MessageBoxA(4), MessageBoxW(4), OpenClipboard(1), CloseClipboard(0), EmptyClipboard(0), SetClipboardData(2), GetClipboardData(1),
        SystemParametersInfoA(4), ChangeDisplaySettingsA(2), WvsprintfA => "wvsprintfA"(3),
        GetKeyboardState(1), ShowCursor(1), SetCursor(1), SetCursorPos(2), GetCapture(0),
        SetCapture(1)[callback], ReleaseCapture(0)[callback], InvalidateRect(3), IsWindowVisible(1), DrawTextA(5),
        SetRect(5), OffsetRect(3), InflateRect(3), IntersectRect(3), UnionRect(3), IsRectEmpty(1), PtInRect(3),
        FindWindowA(2), FindWindowExA(4), GetDC(1), ReleaseDC(2),
        GetCursorPos(1), GetKeyState(1), GetAsyncKeyState(1), GetFocus(0), SetFocus(1),
        RegisterClassA(1), RegisterClassExA(1), GetClassInfoA(3), GetClassInfoExA(3), GetWindowLongA(2), SetWindowLongA(3), DefWindowProcA(4)[callback], ShowWindow(2), GetSystemMetrics(1), EnumDisplaySettingsA(3),
        AdjustWindowRect(3), AdjustWindowRectEx(4), GetClientRect(2), GetWindowRect(2), ClientToScreen(2), ScreenToClient(2), PeekMessageA(5),
        GetMessageA(4), TranslateMessage(1), PostMessageA(4), PostQuitMessage(1), IsWindow(1),
        SetWindowPos(7)[callback], WindowProcA => "CallWindowProcA"(5)[callback], CreateWindowExA(12)[callback], DestroyWindow(1)[callback], UpdateWindow(1),
        DispatchMessageA(1)[callback], SendMessageA(4)[callback]
    }
    "gdi32.dll", stdcall => { CreateDIBSection(6), CreateCompatibleDC(1), DeleteDC(1), SelectObject(2), CreateFontA(14), CreateSolidBrush(1), UnrealizeObject(1), GetSystemPaletteEntries(4), DPtoLP(3),  GetDeviceCaps(2), SelectPalette(3), RealizePalette(1), CreatePalette(1), GetPaletteEntries(4), SetMapMode(2), GetMapMode(1), SetBkMode(2), GetBkMode(1), SetBkColor(2), GetBkColor(1), SetTextColor(2), GetTextColor(1), GetStockObject(1), GetObjectA(3), DeleteObject(1) }
    "ddraw.dll", stdcall => {
    D3DQueryInterface => "D3D_QueryInterface"(3),
    D3DAddRef => "D3D_AddRef"(1),
    D3DRelease => "D3D_Release"(1),
    D3DInitialize => "D3D_Initialize"(2),
    D3DEnumDevices => "D3D_EnumDevices"(3)[callback],
    D3DCreateLight => "D3D_CreateLight"(3),
    D3DCreateMaterial => "D3D_CreateMaterial"(3),
    D3DCreateViewport => "D3D_CreateViewport"(3),
    D3DFindDevice => "D3D_FindDevice"(3),
    D3DCreateDevice => "D3D_CreateDevice"(5),
    D3DCreateVertexBuffer => "D3D_CreateVertexBuffer"(5),
    D3DEnumZBufferFormats => "D3D_EnumZBufferFormats"(4)[callback],
    D3DEvictManagedTextures => "D3D_EvictManagedTextures"(1),
    D3DViewportGetViewport => "D3DViewport_GetViewport"(2),
    D3DViewportSetViewport => "D3DViewport_SetViewport"(2),
    D3DViewportTransformVertices => "D3DViewport_TransformVertices"(5),
    D3DViewportLightElements => "D3DViewport_LightElements"(3),
    D3DViewportSetBackground => "D3DViewport_SetBackground"(2),
    D3DViewportGetBackground => "D3DViewport_GetBackground"(3),
    D3DViewportSetBackgroundDepth => "D3DViewport_SetBackgroundDepth"(2),
    D3DViewportGetBackgroundDepth => "D3DViewport_GetBackgroundDepth"(3),
    D3DViewportClear => "D3DViewport_Clear"(4),
    D3DViewportAddLight => "D3DViewport_AddLight"(2),
    D3DViewportDeleteLight => "D3DViewport_DeleteLight"(2),
    D3DViewportNextLight => "D3DViewport_NextLight"(4),
    D3DViewportGetViewport2 => "D3DViewport_GetViewport2"(2),
    D3DViewportSetViewport2 => "D3DViewport_SetViewport2"(2),
    D3DViewportClear2 => "D3DViewport_Clear2"(7),
    D3DMaterialSetMaterial => "D3DMaterial_SetMaterial"(2),
    D3DMaterialGetMaterial => "D3DMaterial_GetMaterial"(2),
    D3DMaterialGetHandle => "D3DMaterial_GetHandle"(3),
    D3DMaterialReserve => "D3DMaterial_Reserve"(1),
    D3DMaterialUnreserve => "D3DMaterial_Unreserve"(1),
    D3DLightSetLight => "D3DLight_SetLight"(2),
    D3DLightGetLight => "D3DLight_GetLight"(2),
    D3DTextureInitialize => "D3DTexture_Initialize"(3),
    D3DTextureGetHandle => "D3DTexture_GetHandle"(3),
    D3DTexturePaletteChanged => "D3DTexture_PaletteChanged"(3),
    D3DTextureLoad => "D3DTexture_Load"(2),
    D3DTextureUnload => "D3DTexture_Unload"(1),
    D3DDeviceGetCaps => "D3DDevice_GetCaps"(3),
    D3DDeviceGetStats => "D3DDevice_GetStats"(2),
    D3DDeviceAddViewport => "D3DDevice_AddViewport"(2),
    D3DDeviceDeleteViewport => "D3DDevice_DeleteViewport"(2),
    D3DDeviceNextViewport => "D3DDevice_NextViewport"(4),
    D3DDeviceEnumTextureFormats => "D3DDevice_EnumTextureFormats"(3)[callback],
    D3DDeviceBeginScene => "D3DDevice_BeginScene"(1),
    D3DDeviceEndScene => "D3DDevice_EndScene"(1),
    D3DDeviceGetDirect3D => "D3DDevice_GetDirect3D"(2),
    D3DDeviceSetCurrentViewport => "D3DDevice_SetCurrentViewport"(2),
    D3DDeviceGetCurrentViewport => "D3DDevice_GetCurrentViewport"(2),
    D3DDeviceSetRenderTarget => "D3DDevice_SetRenderTarget"(3),
    D3DDeviceGetRenderTarget => "D3DDevice_GetRenderTarget"(2),
    D3DDeviceBegin => "D3DDevice_Begin"(4),
    D3DDeviceBeginIndexed => "D3DDevice_BeginIndexed"(6),
    D3DDeviceVertex => "D3DDevice_Vertex"(2),
    D3DDeviceIndex => "D3DDevice_Index"(2),
    D3DDeviceEnd => "D3DDevice_End"(2),
    D3DDeviceGetRenderState => "D3DDevice_GetRenderState"(3),
    D3DDeviceSetRenderState => "D3DDevice_SetRenderState"(3),
    D3DDeviceGetLightState => "D3DDevice_GetLightState"(3),
    D3DDeviceSetLightState => "D3DDevice_SetLightState"(3),
    D3DDeviceSetTransform => "D3DDevice_SetTransform"(3),
    D3DDeviceGetTransform => "D3DDevice_GetTransform"(3),
    D3DDeviceMultiplyTransform => "D3DDevice_MultiplyTransform"(3),
    D3DDeviceDrawPrimitive => "D3DDevice_DrawPrimitive"(6),
    D3DDeviceDrawIndexedPrimitive => "D3DDevice_DrawIndexedPrimitive"(8),
    D3DDeviceSetClipStatus => "D3DDevice_SetClipStatus"(2),
    D3DDeviceGetClipStatus => "D3DDevice_GetClipStatus"(2),
    D3DDeviceDrawPrimitiveStrided => "D3DDevice_DrawPrimitiveStrided"(6),
    D3DDeviceDrawIndexedPrimitiveStrided => "D3DDevice_DrawIndexedPrimitiveStrided"(8),
    D3DDeviceDrawPrimitiveVB => "D3DDevice_DrawPrimitiveVB"(6),
    D3DDeviceDrawIndexedPrimitiveVB => "D3DDevice_DrawIndexedPrimitiveVB"(6),
    D3DDeviceComputeSphereVisibility => "D3DDevice_ComputeSphereVisibility"(6),
    D3DDeviceGetTexture => "D3DDevice_GetTexture"(3),
    D3DDeviceSetTexture => "D3DDevice_SetTexture"(3),
    D3DDeviceGetTextureStageState => "D3DDevice_GetTextureStageState"(4),
    D3DDeviceSetTextureStageState => "D3DDevice_SetTextureStageState"(4),
    D3DDeviceValidateDevice => "D3DDevice_ValidateDevice"(2),
    D3DDevice2SwapTextureHandles => "D3DDevice2_SwapTextureHandles"(3),
    D3DDevice2EnumTextureFormats => "D3DDevice2_EnumTextureFormats"(3),
    D3DDevice2GetDirect3D => "D3DDevice2_GetDirect3D"(2),
    D3DDevice2DrawPrimitive => "D3DDevice2_DrawPrimitive"(6),
    D3DDevice2DrawIndexedPrimitive => "D3DDevice2_DrawIndexedPrimitive"(8),
        SurfaceGetDC => "Surface_GetDC"(2),
        SurfaceReleaseDC => "Surface_ReleaseDC"(2),
        SurfaceGetDDInterface => "Surface_GetDDInterface"(2),
        PaletteInitialize => "Palette_Initialize"(4),
        DDrawCompact => "DDraw_Compact"(1),
        DDrawFlipToGDISurface => "DDraw_FlipToGDISurface"(1),
        DDrawGetFourCCCodes => "DDraw_GetFourCCCodes"(3),
        DDrawGetGDISurface => "DDraw_GetGDISurface"(2),
        DDrawGetScanLine => "DDraw_GetScanLine"(2),
        DDrawInitialize => "DDraw_Initialize"(2),
        DDrawGetSurfaceFromDC => "DDraw_GetSurfaceFromDC"(3),
        DDrawRestoreAllSurfaces => "DDraw_RestoreAllSurfaces"(1),
        DDrawTestCooperativeLevel => "DDraw_TestCooperativeLevel"(1),
        DDrawGetDeviceIdentifier => "DDraw_GetDeviceIdentifier"(3),

        DirectDrawCreate(3), DirectDrawCreateEx(4),
        DDrawDuplicateSurface => "DDraw_DuplicateSurface"(3),
        SurfaceAddAttachedSurface => "Surface_AddAttachedSurface"(2),
        SurfaceDeleteAttachedSurface => "Surface_DeleteAttachedSurface"(3),
        SurfaceGetColorKey => "Surface_GetColorKey"(3),
        SurfaceInitialize => "Surface_Initialize"(3),
        SurfaceGetUniquenessValue => "Surface_GetUniquenessValue"(2),
        SurfaceGetPrivateData => "Surface_GetPrivateData"(4),
        SurfaceBltBatch => "Surface_BltBatch"(4),
        ClipperGetClipList => "Clipper_GetClipList"(4),
        ClipperInitialize => "Clipper_Initialize"(3),
        ClipperSetClipList => "Clipper_SetClipList"(3),
        DirectDrawEnumerateA(2)[callback], DDrawEnumDisplayModes => "DDraw_EnumDisplayModes"(5)[callback],
        DDrawQueryInterface => "DDraw_QueryInterface"(3),
        DDrawAddRef => "DDraw_AddRef"(1),
        DDrawRelease => "DDraw_Release"(1),
        DDrawCreatePalette => "DDraw_CreatePalette"(5),
        DDrawCreateSurface => "DDraw_CreateSurface"(4),
        DDrawSetCooperativeLevel => "DDraw_SetCooperativeLevel"(3),
        DDrawSetDisplayMode => "DDraw_SetDisplayMode"(4),
        DDrawSetDisplayModeEx => "DDraw_SetDisplayModeEx"(6),
        DDrawGetDisplayMode => "DDraw_GetDisplayMode"(2),
        DDrawRestoreDisplayMode => "DDraw_RestoreDisplayMode"(1),
        DDrawWaitForVerticalBlank => "DDraw_WaitForVerticalBlank"(3),
        SurfaceQueryInterface => "Surface_QueryInterface"(3),
        SurfaceAddRef => "Surface_AddRef"(1),
        SurfaceRelease => "Surface_Release"(1),
        SurfaceBlt => "Surface_Blt"(6),
        SurfaceBltFast => "Surface_BltFast"(6),
        SurfaceFlip => "Surface_Flip"(3),
        SurfaceGetAttachedSurface => "Surface_GetAttachedSurface"(3),
        SurfaceGetCaps => "Surface_GetCaps"(2),
        SurfaceGetSurfaceDesc => "Surface_GetSurfaceDesc"(2),
        SurfaceIsLost => "Surface_IsLost"(1),
        SurfaceLock => "Surface_Lock"(5),
        SurfaceUnlock => "Surface_Unlock"(2),
        SurfaceRestore => "Surface_Restore"(1),
        SurfaceSetColorKey => "Surface_SetColorKey"(3),
        SurfaceSetPalette => "Surface_SetPalette"(2),
        SurfaceGetPixelFormat => "Surface_GetPixelFormat"(2),
        PaletteQueryInterface => "Palette_QueryInterface"(3),
        PaletteAddRef => "Palette_AddRef"(1),
        PaletteRelease => "Palette_Release"(1),
        PaletteGetEntries => "Palette_GetEntries"(5),
        PaletteSetEntries => "Palette_SetEntries"(5),
        PaletteGetCaps => "Palette_GetCaps"(2),
        DDrawCreateClipper => "DDraw_CreateClipper"(4),
        DDrawGetCaps => "DDraw_GetCaps"(3),
        DDrawGetAvailableVidMem => "DDraw_GetAvailableVidMem"(4),
        DDrawGetVerticalBlankStatus => "DDraw_GetVerticalBlankStatus"(2),
        DDrawGetMonitorFrequency => "DDraw_GetMonitorFrequency"(2),
        SurfaceSetClipper => "Surface_SetClipper"(2),
        SurfaceGetClipper => "Surface_GetClipper"(2),
        SurfaceGetPalette => "Surface_GetPalette"(2),
        SurfaceGetBltStatus => "Surface_GetBltStatus"(2),
        SurfaceGetFlipStatus => "Surface_GetFlipStatus"(2),
        ClipperQueryInterface => "Clipper_QueryInterface"(3),
        ClipperAddRef => "Clipper_AddRef"(1),
        ClipperRelease => "Clipper_Release"(1),
        ClipperGetHWnd => "Clipper_GetHWnd"(2),
        ClipperSetHWnd => "Clipper_SetHWnd"(3),
        ClipperIsClipListChanged => "Clipper_IsClipListChanged"(2),
    }
    "dsound.dll", stdcall => {
        DirectSoundCreate(3), DirectSoundCreate8(3), DirectSoundEnumerateA(2), DirectSoundEnumerateW(2), DirectSoundCaptureCreate(3),
        DSDevice0 => "DS_Device_0"(3), DSDevice1 => "DS_Device_1"(1), DSDevice2 => "DS_Device_2"(1), DSDevice3 => "DS_Device_3"(4), DSDevice4 => "DS_Device_4"(2), DSDevice5 => "DS_Device_5"(3), DSDevice6 => "DS_Device_6"(3), DSDevice7 => "DS_Device_7"(1), DSDevice8 => "DS_Device_8"(2), DSDevice9 => "DS_Device_9"(2), DSDevice10 => "DS_Device_10"(2), DSDevice11 => "DS_Device_11"(2),
        DSBuffer0 => "DS_Buffer_0"(3), DSBuffer1 => "DS_Buffer_1"(1), DSBuffer2 => "DS_Buffer_2"(1), DSBuffer3 => "DS_Buffer_3"(2), DSBuffer4 => "DS_Buffer_4"(3), DSBuffer5 => "DS_Buffer_5"(4), DSBuffer6 => "DS_Buffer_6"(2), DSBuffer7 => "DS_Buffer_7"(2), DSBuffer8 => "DS_Buffer_8"(2), DSBuffer9 => "DS_Buffer_9"(2), DSBuffer10 => "DS_Buffer_10"(3), DSBuffer11 => "DS_Buffer_11"(8), DSBuffer12 => "DS_Buffer_12"(4), DSBuffer13 => "DS_Buffer_13"(2), DSBuffer14 => "DS_Buffer_14"(2), DSBuffer15 => "DS_Buffer_15"(2), DSBuffer16 => "DS_Buffer_16"(2), DSBuffer17 => "DS_Buffer_17"(2), DSBuffer18 => "DS_Buffer_18"(1), DSBuffer19 => "DS_Buffer_19"(5), DSBuffer20 => "DS_Buffer_20"(1), DSBuffer21 => "DS_Buffer_21"(4), DSBuffer22 => "DS_Buffer_22"(4), DSBuffer23 => "DS_Buffer_23"(5),
    }
    "shell32.dll", stdcall => { ShellExecuteA(6), ShellExecuteExA(1) }
    "dinput.dll", stdcall => { DirectInputCreateA(4) }
    "imm32.dll", stdcall => { ImmGetContext(1), ImmReleaseContext(2), ImmGetDefaultIMEWnd(1), ImmGetCompositionStringA(4), ImmGetCompositionStringW(4), ImmGetCandidateListA(4) }
    "ole32.dll", stdcall => { CoCreateInstance(5), CoInitialize(1), CoInitializeEx(2), CoUninitialize(0) }
    "winmm.dll", stdcall => {
        WaveOutGetNumDevs => "waveOutGetNumDevs"(0), WaveOutGetDevCapsA => "waveOutGetDevCapsA"(4), WaveOutOpen => "waveOutOpen"(6), WaveOutClose => "waveOutClose"(1), WaveOutPrepareHeader => "waveOutPrepareHeader"(3), WaveOutUnprepareHeader => "waveOutUnprepareHeader"(3), WaveOutWrite => "waveOutWrite"(3), WaveOutPause => "waveOutPause"(1), WaveOutRestart => "waveOutRestart"(1), WaveOutReset => "waveOutReset"(1), WaveOutGetPosition => "waveOutGetPosition"(3), WaveOutSetVolume => "waveOutSetVolume"(2), WaveOutGetVolume => "waveOutGetVolume"(2), MidiOutGetNumDevs => "midiOutGetNumDevs"(0), MidiOutGetDevCapsA => "midiOutGetDevCapsA"(4), MidiOutOpen => "midiOutOpen"(6), MidiOutClose => "midiOutClose"(1), MidiOutShortMsg => "midiOutShortMsg"(2), MidiStreamOpen => "midiStreamOpen"(6), MidiStreamClose => "midiStreamClose"(1), MidiStreamOut => "midiStreamOut"(3), MidiStreamPause => "midiStreamPause"(1), MidiStreamRestart => "midiStreamRestart"(1), MidiStreamStop => "midiStreamStop"(1), MidiStreamSetPosition => "midiStreamSetPosition"(3),
        TimeGetTime => "timeGetTime"(0), TimeGetDevCaps => "timeGetDevCaps"(2), TimeBeginPeriod => "timeBeginPeriod"(1), TimeEndPeriod => "timeEndPeriod"(1),
        TimeSetEvent => "timeSetEvent"(5), TimeKillEvent => "timeKillEvent"(1),
        MmioOpenA => "mmioOpenA"(3), MmioRead => "mmioRead"(3), MmioSeek => "mmioSeek"(3), MmioSetBuffer => "mmioSetBuffer"(4), MmioClose => "mmioClose"(2)
    }
    "user32.dll", cdecl => { WsprintfA => "wsprintfA"(2)[variadic] }
    "version.dll", stdcall => { GetFileVersionInfoSizeA(2), GetFileVersionInfoA(4), VerQueryValueA(4) }

}

pub fn resolve(dll: &str, name: &str) -> Option<Api> {
    let normalized = dll.to_ascii_lowercase();
    let dll = normalized.strip_suffix(".dll").unwrap_or(&normalized);
    APIS.iter()
        .find(|api| api.dll[..api.dll.len() - 4].eq_ignore_ascii_case(dll) && api.name == name)
        .copied()
}

pub fn manifest() -> &'static [Api] {
    APIS
}

/// Stable native thunk descriptors for the host linker; no runtime pointers escape.
pub fn manifest_json() -> String {
    let entries: Vec<_> = manifest()
        .iter()
        .map(|api| {
            serde_json::json!({
                "dll": api.dll, "name": api.name, "id": api.id, "arity": api.arity, "stackArgs": api.stack_args, "callback": api.callback, "exception": api.exception, "variadic": api.variadic, "clock": api.clock, "wallClock": api.wall_clock, "media": api.media, "storage": api.storage, "schedule": api.schedule, "wake": api.wake
            })
        })
        .collect();
    serde_json::Value::Array(entries).to_string()
}
