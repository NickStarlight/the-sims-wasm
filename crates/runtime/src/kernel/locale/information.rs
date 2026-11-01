//! Compatibility locale identities and enumeration ported from the working runtime.
//! Locale-specific languages use the legacy table; formatting remains the legacy synthetic US profile.
use super::*;
use crate::CallOutcome;

impl Kernel {
    pub(in crate::kernel) fn valid_locale(&mut self, args: &[u32]) -> u32 {
        if !matches!(args[1], 1 | 2) {
            return self.fail(1004); // LCID_INSTALLED or LCID_SUPPORTED, never both.
        }
        // System/user defaults resolve to the same installed English-US profile.
        u32::from(
            matches!(args[0], 0x400 | 0x800)
                || super::data::LANGUAGES.iter().any(|(id, _)| *id == args[0]),
        )
    }

    pub(in crate::kernel) fn enumerate_locales<M: GuestMemory>(
        &mut self,
        process: &mut Process<M>,
        args: &[u32],
    ) -> Result<CallOutcome> {
        if args[0] == 0 {
            return Ok(CallOutcome::Return {
                value: self.fail(87),
            });
        }
        if args[1] & !7 != 0 || args[1] & 3 == 3 {
            return Ok(CallOutcome::Return {
                value: self.fail(1004),
            });
        }
        if args[1] == 4 {
            // There are no installed alternate sort orders.
            return Ok(CallOutcome::Return { value: 1 });
        }
        let pointer = match self.locale_name {
            Some(pointer) => pointer,
            None => {
                let pointer = process.heap_alloc(super::data::LANGUAGES.len() as u32 * 9)?;
                self.locale_name = Some(pointer);
                pointer
            }
        };
        let mut calls = Vec::new();
        for (index, (locale, _)) in super::data::LANGUAGES.iter().enumerate() {
            let address = pointer + index as u32 * 9;
            process.write(address, format!("{locale:08x}\0").as_bytes())?;
            calls.push((args[0], vec![address]));
        }
        // The working bridge tested AL and returned FALSE when enumeration stopped.
        self.callbacks.sequence_with_stop(calls, true, 1, 0xff, 0)
    }

    pub(in crate::kernel) fn locale_info<M: GuestMemory>(
        &mut self,
        process: &mut Process<M>,
        args: &[u32],
        wide: bool,
    ) -> Result<u32> {
        if args[1] & 0x1fff0000 != 0 {
            return Ok(self.fail(1004));
        }
        if (args[3] as i32) < 0 {
            return Ok(self.fail(87));
        }
        // NOUSEROVERRIDE and USE_CP_ACP are equivalent for this fixed CP1252 locale.
        let kind = args[1] & 0xffff;
        let locale = args[0] & 0xffff;
        let language = super::data::LANGUAGES
            .iter()
            .find(|(id, _)| *id == locale)
            .or_else(|| {
                super::data::LANGUAGES
                    .iter()
                    .find(|(id, _)| *id == locale & 0x3ff)
            })
            .map(|(_, name)| *name);
        let value = match (language, kind) {
            (Some(name), 0x1001 | 2 | 4) => Some((name.to_owned(), None)),
            (Some(_), 1) => Some((format!("{locale:08x}"), Some(locale))),
            (Some(_), 9) => Some((format!("{:04x}", locale & 0x3ff), Some(locale & 0x3ff))),
            _ => super::profile::value(kind).map(|(text, number)| (text.to_owned(), number)),
        };
        let Some((text, number)) = value else {
            return Ok(self.fail(87));
        };
        let output = if args[1] & 0x20000000 != 0 {
            let Some(number) = number else {
                return Ok(self.fail(1004));
            };
            number.to_le_bytes().to_vec()
        } else if wide {
            text.encode_utf16()
                .chain([0])
                .flat_map(u16::to_le_bytes)
                .collect()
        } else {
            text.bytes().chain([0]).collect()
        };
        let count = output.len() as u32 / if wide { 2 } else { 1 };
        if args[3] == 0 {
            return Ok(count);
        }
        if args[2] == 0 {
            return Ok(self.fail(87));
        }
        if args[3] < count {
            return Ok(self.fail(122));
        }
        process.write(args[2], &output)?;
        Ok(count)
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/kernel/locale/information.rs"]
mod tests;
