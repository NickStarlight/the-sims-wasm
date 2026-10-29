//! Original DIB validation/layout; bitmap storage lives only in checked guest memory.
use super::*;
use crate::{Error, GuestMemory, Process, Result};
#[derive(Clone)]
pub(super) struct Bitmap {
    address: u32,
    width: u32,
    height: u32,
    stride: u32,
    bpp: u32,
}
impl Bitmap {
    pub fn record(&self) -> Vec<u8> {
        let mut b = vec![0; 44];
        for (o, v) in [
            (4, self.width),
            (8, self.height),
            (12, self.stride),
            (20, self.address),
        ] {
            b[o..o + 4].copy_from_slice(&v.to_le_bytes());
        }
        b[16..18].copy_from_slice(&1u16.to_le_bytes());
        b[18..20].copy_from_slice(&(self.bpp as u16).to_le_bytes());
        if self.bpp == 16 {
            for (i, v) in [0x7c00u32, 0x3e0, 0x1f].iter().enumerate() {
                b[24 + i * 4..28 + i * 4].copy_from_slice(&v.to_le_bytes());
            }
        }
        b
    }
}
impl Gdi {
    pub fn create_dib<M: GuestMemory>(&mut self, p: &mut Process<M>, a: &[u32]) -> Result<u32> {
        if a[3] != 0 {
            p.write(a[3], &[0; 4])?;
        }
        if a[1] == 0 || a[2] > 1 {
            return Ok(0);
        }
        let mut head = [0; 40];
        p.read(a[1], &mut head[..4])?;
        let size = u32::from_le_bytes(head[..4].try_into().unwrap());
        let (width, height, planes, bpp, compression) = match size {
            12 => {
                p.read(a[1], &mut head[..12])?;
                let w = |i| u16::from_le_bytes(head[i..i + 2].try_into().unwrap()) as u32;
                (w(4), w(6), w(8), w(10), 0)
            }
            40.. => {
                p.read(a[1], &mut head)?;
                let w = |i| u32::from_le_bytes(head[i..i + 4].try_into().unwrap());
                (
                    w(4),
                    (w(8) as i32).unsigned_abs(),
                    u16::from_le_bytes([head[12], head[13]]) as u32,
                    u16::from_le_bytes([head[14], head[15]]) as u32,
                    w(16),
                )
            }
            _ => return Ok(0),
        };
        // The original profile rejects planes*bpp above16, including24/32-bit DIBs.
        if width as i32 <= 0
            || height == 0
            || planes == 0
            || planes.saturating_mul(bpp) > 16
            || ![1, 4, 8, 16, 24, 32].contains(&bpp)
            || !(compression == 0 || (compression == 3 && matches!(bpp, 16 | 32)))
        {
            return Ok(0);
        }
        if compression == 3 {
            let mut masks = [0; 12];
            p.read(a[1].checked_add(40).ok_or(Error::InvalidRange)?, &mut masks)?;
            if masks
                .as_chunks::<4>()
                .0
                .iter()
                .any(|b| u32::from_le_bytes(*b) == 0)
            {
                return Ok(0);
            }
        }
        let stride = (u64::from(width) * u64::from(bpp)).div_ceil(32) * 4;
        let bytes = stride * height as u64;
        if bytes > 64 * 1024 * 1024 {
            return Ok(0);
        }
        let address = p.virtual_alloc(0, bytes as u32)?;
        let handle = self.new_object(objects::Object::Bitmap(Bitmap {
            address,
            width,
            height,
            stride: stride as u32,
            bpp,
        }))?;
        if a[3] != 0 {
            p.write(a[3], &address.to_le_bytes())?;
        }
        Ok(handle)
    }
    pub fn bitmap_record(&self, handle: u32) -> Option<Vec<u8>> {
        match self.objects.get(&handle) {
            Some(objects::Object::Bitmap(bitmap)) => Some(bitmap.record()),
            _ => None,
        }
    }
    pub fn delete_owned_object<M: GuestMemory>(
        &mut self,
        p: &mut Process<M>,
        handle: u32,
    ) -> Result<bool> {
        let address = match self.objects.get(&handle) {
            Some(objects::Object::Bitmap(bitmap)) => Some(bitmap.address),
            _ => None,
        };
        if !self.delete_object(handle) {
            return Ok(false);
        }
        if let Some(address) = address {
            p.virtual_free(address)?;
        }
        Ok(true)
    }
}
