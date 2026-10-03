//! One checked index decoder shared by ordinary and instanced indexed draws.
use super::{Kind, Result, WebGl, gl};
impl WebGl {
    pub(super) fn index_size(&self, kind: u32) -> Result<usize> {
        match kind {
            gl::UNSIGNED_BYTE => Ok(1),
            gl::UNSIGNED_SHORT => Ok(2),
            gl::UNSIGNED_INT if self.extensions.uint_indices => Ok(4),
            _ => Err(gl::INVALID_ENUM),
        }
    }
    pub(super) fn maximum_index(&self, count: usize, size: usize, offset: usize) -> Result<u32> {
        let buffer = self.objects.get(self.element_buffer, Kind::Buffer)?;
        let end = offset
            .checked_add(count.checked_mul(size).ok_or(gl::INVALID_OPERATION)?)
            .ok_or(gl::INVALID_OPERATION)?;
        let indices = buffer.bytes.get(offset..end).ok_or(gl::INVALID_OPERATION)?;
        let maximum = match size {
            1 => indices.iter().copied().map(u32::from).max(),
            2 => indices
                .chunks_exact(2)
                .map(|v| u32::from(u16::from_ne_bytes([v[0], v[1]])))
                .max(),
            4 => indices
                .chunks_exact(4)
                .map(|v| u32::from_ne_bytes([v[0], v[1], v[2], v[3]]))
                .max(),
            _ => return Err(gl::INVALID_ENUM),
        };
        maximum.ok_or(gl::INVALID_OPERATION)
    }
}
