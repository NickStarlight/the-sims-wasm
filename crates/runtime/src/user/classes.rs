use super::*;

pub(super) struct Class {
    pub atom: u32,
    pub name: Vec<u8>,
    pub instance: u32,
    pub procedure: u32,
    fields: [u32; 10],
    pub window_extra: usize,
    small_icon: u32,
}
impl User {
    pub(super) fn register<M: GuestMemory>(
        &mut self,
        process: &mut Process<M>,
        address: u32,
        extended: bool,
    ) -> Result<u32> {
        let (mut fields, small_icon) = if extended {
            let fields = words::<_, 12>(process, address)?;
            if fields[0] != 48 {
                return Ok(self.fail(87));
            }
            (fields[1..11].try_into().unwrap(), fields[11])
        } else {
            (words::<_, 10>(process, address)?, 0)
        };
        let name = process.read_c_string(fields[9], 256)?;
        if name.is_empty() || fields[1] == 0 || fields[2] > 65536 || fields[3] > 65536 {
            return Ok(self.fail(87));
        }
        if self
            .classes
            .values()
            .any(|class| class.instance == fields[4] && class.name.eq_ignore_ascii_case(&name))
        {
            return Ok(self.fail(1410));
        }
        if self.classes.len() >= MAX_WINDOWS {
            return Ok(self.fail(8));
        }
        // A registration owns its strings: callers may reuse both the WNDCLASS
        // structure and its pointed-to buffers as soon as this call returns.
        let menu = if fields[8] > 0xffff {
            Some(process.read_c_string(fields[8], 4096)?)
        } else {
            None
        };
        let mut strings = name.clone();
        strings.push(0);
        let menu_offset = strings.len() as u32;
        if let Some(menu) = &menu {
            strings.extend_from_slice(menu);
            strings.push(0);
        }
        let storage = process.heap_alloc(strings.len() as u32)?;
        if let Err(error) = process.write(storage, &strings) {
            process.heap_free(storage)?;
            return Err(error);
        }
        fields[9] = storage;
        if menu.is_some() {
            fields[8] = storage + menu_offset;
        }
        let atom = self.next_atom + 1;
        self.classes.insert(
            atom,
            Class {
                atom,
                name,
                instance: fields[4],
                procedure: fields[1],
                window_extra: fields[3] as usize,
                fields,
                small_icon,
            },
        );
        self.next_atom = atom;
        Ok(atom)
    }
    pub(super) fn class_info<M: GuestMemory>(
        &mut self,
        process: &mut Process<M>,
        args: &[u32],
        extended: bool,
    ) -> Result<u32> {
        let Some(class) = self.class(process, args[1], args[0])? else {
            return Ok(self.fail(1411)); // ERROR_CLASS_DOES_NOT_EXIST
        };
        if extended {
            let mut fields = [0; 12];
            fields[0] = 48;
            fields[1..11].copy_from_slice(&class.fields);
            fields[11] = class.small_icon;
            write_words(process, args[2], &fields)?;
        } else {
            write_words(process, args[2], &class.fields)?;
        }
        Ok(1)
    }
    pub(super) fn class<M: GuestMemory>(
        &self,
        process: &Process<M>,
        name: u32,
        instance: u32,
    ) -> Result<Option<&Class>> {
        if name <= 0xffff {
            return Ok(self
                .classes
                .get(&name)
                .filter(|class| class.instance == instance || class.instance == 0));
        }
        let name = process.read_c_string(name, 256)?;
        Ok(self.classes.values().find(|class| {
            (class.instance == instance || class.instance == 0)
                && class.name.eq_ignore_ascii_case(&name)
        }))
    }
}

#[cfg(test)]
#[path = "../../tests/unit/user/classes.rs"]
mod tests;
