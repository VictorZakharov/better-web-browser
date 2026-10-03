//! Numeric buffer offsets are distinct from owned CPU pixel pointers.
use super::core_buffers::{PIXEL_PACK, PIXEL_UNPACK};
use super::pixel_layout::{Direction, Store};
use super::{ApiVersion, Command, Kind, MAX_UPLOAD_BYTES, Result, WebGl, gl};
use serde_json::Value;
use std::ffi::c_void;

fn type_alignment(kind: u32) -> Result<usize> {
    Ok(match kind {
        gl::BYTE | gl::UNSIGNED_BYTE => 1,
        gl::SHORT | gl::UNSIGNED_SHORT | 0x140b | 0x8033 | 0x8034 | 0x8363 => 2,
        gl::INT | gl::UNSIGNED_INT | gl::FLOAT | 0x8368 | 0x8c3b | 0x8c3e | 0x84fa => 4,
        0x8dad => 8,
        _ => return Err(gl::INVALID_ENUM),
    })
}
impl WebGl {
    fn pixel_buffer_offset(
        &self,
        c: &Command,
        target: u32,
        index: usize,
        size: usize,
        kind: u32,
    ) -> Result<usize> {
        let id = *self.core_buffer_bindings.get(&target).unwrap_or(&0);
        let buffer = self.objects.get(id, Kind::Buffer)?;
        let offset =
            c.i.get(index)
                .and_then(|value| usize::try_from(*value).ok())
                .ok_or(gl::INVALID_VALUE)?;
        if !offset.is_multiple_of(type_alignment(kind)?) {
            return Err(gl::INVALID_OPERATION);
        }
        if offset
            .checked_add(size)
            .is_none_or(|end| end > buffer.bytes.len())
        {
            return Err(gl::INVALID_OPERATION);
        }
        Ok(offset)
    }
    pub(super) fn unpack_pointer(
        &self,
        c: &Command,
        bytes: Option<&[u8]>,
        from_buffer: bool,
        index: usize,
        size: usize,
        kind: u32,
    ) -> Result<*const c_void> {
        if !from_buffer {
            return Ok(bytes.map_or(std::ptr::null(), |data| data.as_ptr().cast()));
        }
        if bytes.is_some() {
            return Err(gl::INVALID_OPERATION);
        }
        // The pointer is never dereferenced by Rust. With the validated native
        // PBO binding it denotes a bounded byte offset, including offset zero.
        Ok(self.pixel_buffer_offset(c, PIXEL_UNPACK, index, size, kind)? as *const c_void)
    }
    pub(super) fn read_pixels_to_buffer(
        &mut self,
        c: &Command,
        bytes: Option<&[u8]>,
    ) -> Result<Value> {
        if self.options.api != ApiVersion::Two || bytes.is_some() {
            return Err(gl::INVALID_OPERATION);
        }
        let (width, height) = (c.n(2)?, c.n(3)?);
        if width < 0 || height < 0 {
            return Err(gl::INVALID_VALUE);
        }
        let (format, kind) = (c.u(4)?, c.u(5)?);
        let pixel_bytes = self.core_read_pair(format, kind)?;
        let layout = Store::native(self.options.api, Direction::Pack)?.layout(
            width as usize,
            height as usize,
            1,
            pixel_bytes,
            false,
        )?;
        if layout.size > MAX_UPLOAD_BYTES {
            return Err(gl::OUT_OF_MEMORY);
        }
        let offset = self.pixel_buffer_offset(c, PIXEL_PACK, 6, layout.size, kind)?;
        unsafe {
            gl::ReadPixels(
                c.n(0)?,
                c.n(1)?,
                width,
                height,
                format,
                kind,
                offset as *mut c_void,
            )
        };
        self.driver_result()?;
        // This deliberately does not synchronously map the GPU buffer. The
        // CPU mirror is used for element-index checks; a pixel buffer cannot
        // belong to that class. getBufferSubData reads the native truth on demand.
        Ok(Value::Null)
    }
}
