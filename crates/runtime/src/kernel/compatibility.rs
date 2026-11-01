//! Original platform compatibility contracts; unsupported services retain their old results.
//! Reference: runtime-rs/src/kernel/{compat,environment,virtual_memory} and winapi/kernel32.ts.
use super::*;
impl Kernel {
    pub(super) fn compatibility<M: GuestMemory>(
        &mut self,
        p: &mut Process<M>,
        call: Call,
        a: &[u32],
    ) -> Result<u32> {
        Ok(match call {
            Call::DirectInputCreateA => {
                // Complete's DirectX probe checks this export but never invokes
                // it; gameplay input uses the existing Win32 message APIs.
                // Device creation remains explicitly unsupported, not a fake
                // successful DirectInput object.
                if a[2] != 0 {
                    p.write(a[2], &0u32.to_le_bytes())?;
                }
                0x8000_4001 // E_NOTIMPL / DIERR_UNSUPPORTED
            }
            Call::ShellExecuteA => 42, // Original launcher acknowledgement; no host process is launched.
            Call::ImmGetContext
            | Call::ImmGetDefaultIMEWnd
            | Call::ImmGetCompositionStringA
            | Call::ImmGetCompositionStringW
            | Call::ImmGetCandidateListA => 0,
            Call::ImmReleaseContext
            | Call::HeapDestroy
            | Call::SetStdHandle
            | Call::QueueUserAPC
            | Call::SetFileAttributesA
            | Call::RemoveDirectoryA
            | Call::TerminateProcess
            | Call::GetProcessWorkingSetSize
            | Call::SetProcessWorkingSetSize
            | Call::SetConsoleCtrlHandler
            | Call::UnhandledExceptionFilter => 1,
            Call::GetProcessHeaps
            | Call::IsBadReadPtr
            | Call::IsBadWritePtr
            | Call::IsBadCodePtr
            | Call::CreateProcessA
            | Call::OpenProcess
            | Call::Lclose
            | Call::ExitProcess
            | Call::FatalAppExitA
            | Call::OutputDebugStringA => 0,
            Call::GetDriveTypeA => 3,
            Call::SetEnvironmentVariableA | Call::SetEnvironmentVariableW => {
                if a[0] == 0 {
                    self.fail(87)
                } else {
                    1
                }
            }
            Call::GetExitCodeProcess => {
                if a[1] != 0 {
                    p.write(a[1], &259u32.to_le_bytes())?;
                }
                1
            }
            Call::GetSystemDirectoryA => {
                let value = b"C:\\WINDOWS\\SYSTEM\0";
                if a[0] == 0 || a[1] == 0 {
                    0
                } else if a[1] < value.len() as u32 {
                    value.len() as u32
                } else {
                    p.write(a[0], value)?;
                    value.len() as u32 - 1
                }
            }
            Call::GetSystemInfo => {
                let mut bytes = [0; 36];
                for (i, v) in [0x10000u32, 0x10000, 0x7ffeffff, 1, 1, 586, 0x10000]
                    .iter()
                    .enumerate()
                {
                    bytes[4 + i * 4..8 + i * 4].copy_from_slice(&v.to_le_bytes());
                }
                p.write(a[0], &bytes)?;
                0
            }
            Call::VirtualQuery => {
                if a[1] == 0 || a[2] < 28 {
                    0
                } else {
                    let mut bytes = [0; 28];
                    for (i, v) in [a[0], a[0], 4, 0x10000, 0x1000, 4, 0x40000]
                        .iter()
                        .enumerate()
                    {
                        bytes[i * 4..i * 4 + 4].copy_from_slice(&v.to_le_bytes());
                    }
                    p.write(a[1], &bytes)?;
                    28
                }
            }
            Call::OpenFile => {
                if a[1] != 0 {
                    p.write(a[1], &[136, 1, 2, 0])?;
                }
                u32::MAX
            }
            _ => return Err(Error::Host("invalid compatibility routing".into())),
        })
    }
}
