//! VERSION APIs read actual preloaded PE resources; absent data is a Windows error.
mod block;
mod resource;
use super::*;

fn read_string<M: GuestMemory>(process: &Process<M>, pointer: u32) -> Result<String> {
    let mut output = String::new();
    for offset in 0..32768u32 {
        let mut byte = [0];
        process.read(
            pointer.checked_add(offset).ok_or(Error::InvalidRange)?,
            &mut byte,
        )?;
        if byte[0] == 0 {
            return Ok(output);
        }
        output.push(
            char::from_u32(u32::from(text::decode_byte(byte[0]))).ok_or(Error::InvalidRange)?,
        );
    }
    Err(Error::Host("unterminated version API string".into()))
}
impl Kernel {
    pub(super) fn version<M: GuestMemory>(
        &mut self,
        process: &mut Process<M>,
        call: Call,
        args: &[u32],
    ) -> Result<u32> {
        if call == Call::VerQueryValueA {
            return self.version_query(process, args);
        }
        if args[0] == 0 {
            return Ok(self.fail(87));
        }
        if call == Call::GetFileVersionInfoSizeA && args[1] != 0 {
            process.write(args[1], &0u32.to_le_bytes())?;
        }
        let path = read_string(process, args[0])?;
        let packed = match self.files.file_bytes(&path)? {
            Some(bytes) => resource::resource(&bytes)
                .and_then(block::unpack)
                .map(|(_, packed)| packed),
            None => Err(2),
        };
        let packed = match packed {
            Ok(bytes) => bytes,
            Err(error) => return Ok(self.fail(error)),
        };
        if call == Call::GetFileVersionInfoSizeA {
            return Ok(packed.len() as u32);
        }
        if args[3] == 0 || args[2] == 0 {
            return Ok(self.fail(87));
        }
        // GetFileVersionInfo truncates to the caller's buffer length.
        // https://learn.microsoft.com/windows/win32/api/winver/nf-winver-getfileversioninfoa
        let length = (args[2] as usize).min(packed.len());
        process.write(args[3], &packed[..length])?;
        Ok(1)
    }

    fn version_query<M: GuestMemory>(
        &mut self,
        process: &mut Process<M>,
        args: &[u32],
    ) -> Result<u32> {
        if args.contains(&0) {
            return Ok(self.fail(87));
        }
        process.write(args[2], &0u32.to_le_bytes())?;
        process.write(args[3], &0u32.to_le_bytes())?;
        let mut length = [0; 2];
        process.read(args[0], &mut length)?;
        let mut raw = vec![0; u16::from_le_bytes(length) as usize];
        process.read(args[0], &mut raw)?;
        let (root, _) = match block::unpack(&raw) {
            Ok(parsed) => parsed,
            Err(error) => return Ok(self.fail(error)),
        };
        let path = read_string(process, args[1])?;
        let mut node = &root;
        for part in path.split('\\').filter(|part| !part.is_empty()) {
            let Some(child) = node
                .children
                .iter()
                .find(|child| child.key.eq_ignore_ascii_case(part))
            else {
                return Ok(self.fail(1813));
            };
            node = child;
        }
        if node.size == 0 {
            return Ok(self.fail(1813));
        }
        let address = args[0]
            .checked_add(node.value as u32)
            .ok_or(Error::InvalidRange)?;
        let mut check = vec![0; node.size];
        process.read(address, &mut check)?;
        process.write(args[2], &address.to_le_bytes())?;
        process.write(args[3], &(node.size as u32).to_le_bytes())?;
        Ok(1)
    }
}

#[cfg(test)]
#[path = "../../tests/unit/kernel/version.rs"]
mod tests;
