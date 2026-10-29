//! Existing limited USER32 contracts, ported without inventing resource/rendering behavior.
//! Reference: old runtime-rs/src/user/compat and src/runtime/winapi/user32.ts.
use super::*;

pub(super) fn call<M: GuestMemory>(
    process: &mut Process<M>,
    name: &str,
    args: &[u32],
) -> Result<u32> {
    let value = match name {
        // The old backend used opaque fixed cursor identities; it never decoded the file.
        "LoadCursorFromFileA" => 0x30010,
        "DestroyCursor" => 1,
        "MoveWindow" | "SetForegroundWindow" => 1,
        "GetParent" => 0,
        // Paint contracts do not fill PAINTSTRUCT or rasterize pixels in the old implementation.
        "BeginPaint" => 0x20000,
        "EndPaint" | "FillRect" => 1,
        "MsgWaitForMultipleObjects" => 258,
        "CreateMenu" | "CreatePopupMenu" | "TrackPopupMenu" | "GetMenu" => 0,
        "AppendMenuA" | "DestroyMenu" => 1,
        "MessageBoxA" | "MessageBoxW" => 1,
        "OpenClipboard" | "CloseClipboard" | "EmptyClipboard" => 1,
        "SetClipboardData" | "GetClipboardData" => 0,
        "SystemParametersInfoA" => {
            if args[0] == 0x30 && args[2] != 0 {
                write_words(process, args[2], &[0, 0, 800, 600])?;
            }
            1
        }
        "ChangeDisplaySettingsA" => {
            if args[0] != 0 {
                process.write(
                    args[0].checked_add(0x26).ok_or(Error::InvalidRange)?,
                    &[0, 0],
                )?;
            }
            0
        }
        _ => return Err(Error::Host(format!("unsupported USER32 service {name}"))),
    };
    Ok(value)
}

#[cfg(test)]
#[path = "../../tests/unit/user/compatibility.rs"]
mod tests;
