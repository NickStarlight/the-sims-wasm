//! COM apartment registration belongs to the calling guest thread; object activation is separate.
use super::*;

pub(super) struct Apartment {
    single_threaded: bool,
    references: u32,
}

impl Kernel {
    pub(super) fn com_initialize(&mut self, call: Call, args: &[u32]) -> Result<u32> {
        if call == Call::CoUninitialize {
            let apartment = &mut self.thread().apartment;
            if let Some(state) = apartment {
                state.references -= 1;
                if state.references == 0 {
                    *apartment = None;
                }
            }
            return Ok(0); // void; extra uninitialization cannot underflow the count.
        }
        let flags = if call == Call::CoInitialize {
            2
        } else {
            args[1]
        };
        if args[0] != 0 || flags & !0x0e != 0 {
            return Ok(0x8007_0057); // E_INVALIDARG
        }
        // DISABLE_OLE1DDE and SPEED_OVER_MEMORY are implementation preferences;
        // they do not change the apartment model or its compatibility rules.
        let single_threaded = flags & 2 != 0;
        let apartment = &mut self.thread().apartment;
        if let Some(state) = apartment {
            if state.single_threaded != single_threaded {
                return Ok(0x8001_0106); // RPC_E_CHANGED_MODE; failed calls do not increment.
            }
            state.references = state.references.checked_add(1).ok_or(Error::InvalidRange)?;
            Ok(1) // S_FALSE; still requires a balancing CoUninitialize.
        } else {
            *apartment = Some(Apartment {
                single_threaded,
                references: 1,
            });
            Ok(0) // S_OK
        }
    }
}

#[cfg(test)]
#[path = "../../tests/unit/kernel/com.rs"]
mod tests;
