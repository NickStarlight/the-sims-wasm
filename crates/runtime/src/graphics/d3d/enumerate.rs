use super::*;
impl Graphics {
    pub(crate) fn d3d_enumerate<M: GuestMemory>(
        &mut self,
        p: &mut Process<M>,
        name: &str,
        a: &[u32],
    ) -> Result<Vec<(u32, Vec<u32>)>> {
        let (callback, context, records) = match name {
            "D3D_EnumDevices" => {
                let mut bytes = vec![0; 584];
                for (i, v) in interfaces::RGB.iter().enumerate() {
                    bytes[i * 4..i * 4 + 4].copy_from_slice(&v.to_le_bytes());
                }
                bytes[16..30].copy_from_slice(b"RGB Emulation\0");
                bytes[40..65].copy_from_slice(b"Software triangle device\0");
                bytes[80..332].copy_from_slice(&resources::caps(false, 252));
                bytes[332..584].copy_from_slice(&resources::caps(true, 252));
                (a[1], a[2], vec![bytes])
            }
            "D3D_EnumZBufferFormats" => {
                let mut bytes = vec![0; 32];
                for (i, value) in [32u32, 0x400, 0, 16, 0, 0xffff, 0, 0].iter().enumerate() {
                    bytes[i * 4..i * 4 + 4].copy_from_slice(&value.to_le_bytes());
                }
                (a[2], a[3], vec![bytes])
            }
            "D3DDevice_EnumTextureFormats" => {
                let formats = [
                    [0x40u32, 16, 0xf800, 0x7e0, 0x1f, 0],
                    [0x41, 16, 0x7c00, 0x3e0, 0x1f, 0x8000],
                    [0x41, 16, 0xf00, 0xf0, 0xf, 0xf000],
                    [0x40, 32, 0xff0000, 0xff00, 0xff, 0],
                    [0x41, 32, 0xff0000, 0xff00, 0xff, 0xff000000],
                ];
                let records = formats
                    .iter()
                    .map(|format| {
                        let mut bytes = vec![0; 32];
                        bytes[..4].copy_from_slice(&32u32.to_le_bytes());
                        bytes[4..8].copy_from_slice(&format[0].to_le_bytes());
                        for (i, value) in format[1..].iter().enumerate() {
                            bytes[12 + i * 4..16 + i * 4].copy_from_slice(&value.to_le_bytes());
                        }
                        bytes
                    })
                    .collect();
                (a[1], a[2], records)
            }
            _ => return Err(Error::Host(format!("unknown Direct3D enumeration {name}"))),
        };
        if callback == 0 {
            return Err(Error::Host("null Direct3D enumeration callback".into()));
        }
        let mut calls = Vec::new();
        for bytes in records {
            let address = p.heap_alloc(bytes.len() as u32)?;
            p.write(address, &bytes)?;
            self.enumeration_buffers.push(address);
            let args = if name == "D3D_EnumDevices" {
                vec![
                    address,
                    address + 40,
                    address + 16,
                    address + 80,
                    address + 332,
                    context,
                ]
            } else {
                vec![address, context]
            };
            calls.push((callback, args));
        }
        Ok(calls)
    }
}
