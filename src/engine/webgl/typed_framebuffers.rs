//! Typed clears/read pairs retain integer signedness and HDR color values.
use super::framebuffer_guard::READ;
use super::{ApiVersion, Command, Result, WebGl, gl};
use serde_json::Value;
const COLOR: u32 = 0x1800;
const DEPTH: u32 = 0x1801;
const STENCIL: u32 = 0x1802;

impl WebGl {
    pub(super) fn typed_clear(&mut self, c: &Command) -> Result<Value> {
        if self.options.api != ApiVersion::Two {
            return Err(gl::INVALID_OPERATION);
        }
        let buffer = c.u(0)?;
        let index = c.n(1)?;
        let valid = match c.op.as_str() {
            "clearBufferfv" => [COLOR, DEPTH].contains(&buffer),
            "clearBufferiv" => [COLOR, STENCIL].contains(&buffer),
            "clearBufferuiv" => buffer == COLOR,
            "clearBufferfi" => buffer == 0x84f9,
            _ => false,
        };
        if !valid {
            return Err(gl::INVALID_ENUM);
        }
        if index < 0
            || (buffer != COLOR && index != 0)
            || (buffer == COLOR && index as u32 >= self.extensions.max_draw_buffers)
        {
            return Err(gl::INVALID_VALUE);
        }
        self.validate_framebuffer()?;
        // ANGLE's enabled WebGL compatibility validator checks the attachment's
        // component type against the chosen clear overload before accessing it.
        // The arrays below always have that overload's exact native width.
        let core = self.core.as_ref().ok_or(gl::INVALID_OPERATION)?;
        let count = if buffer == COLOR { 4 } else { 1 };
        match c.op.as_str() {
            "clearBufferfv" => {
                let mut values = [0.; 4];
                for (i, value) in values.iter_mut().enumerate().take(count) {
                    *value = c.float(i)?;
                }
                unsafe { (core.clear_float)(buffer, index, values.as_ptr()) };
            }
            "clearBufferiv" => {
                let mut values = [0; 4];
                for (i, value) in values.iter_mut().enumerate().take(count) {
                    *value = c.n(i + 2)?;
                }
                unsafe { (core.clear_signed)(buffer, index, values.as_ptr()) };
            }
            "clearBufferuiv" => {
                let mut values = [0; 4];
                for (i, value) in values.iter_mut().enumerate() {
                    *value = c.u(i + 2)?;
                }
                unsafe { (core.clear_unsigned)(buffer, index, values.as_ptr()) };
            }
            "clearBufferfi" => unsafe {
                (core.clear_depth_stencil)(buffer, index, c.float(0)?, c.n(2)?)
            },
            _ => unreachable!(),
        }
        self.driver_result()?;
        Ok(Value::Null)
    }

    pub(super) fn core_read_pair(&mut self, format: u32, kind: u32) -> Result<usize> {
        // GLES3 §4.3.2 distinguishes an unknown read enum from a known pair
        // unsupported by the selected color attachment. Upload-only depth,
        // luminance and packed-depth enums must not reach the latter check.
        super::core_texture_formats::validate_read_enums(format, kind)?;
        self.validate_read_framebuffer()?;
        if self.normalized_read_pair(format, kind)? {
            return Ok(8);
        }
        let component = if self.read_framebuffer == 0 {
            0x8c17
        } else {
            let route = self
                .objects
                .get(self.read_framebuffer, super::Kind::Framebuffer)?
                .read_buffer;
            if route == gl::NONE {
                return Err(gl::INVALID_OPERATION);
            }
            let mut component = 0;
            unsafe { gl::GetFramebufferAttachmentParameteriv(READ, route, 0x8211, &mut component) };
            self.driver_result()?;
            component as u32
        };
        let mandatory = match component {
            gl::INT => (super::core_texture_formats::RGBA_INTEGER, gl::INT, 16),
            gl::UNSIGNED_INT => (
                super::core_texture_formats::RGBA_INTEGER,
                gl::UNSIGNED_INT,
                16,
            ),
            gl::FLOAT => (gl::RGBA, gl::FLOAT, 16),
            _ => (gl::RGBA, gl::UNSIGNED_BYTE, 4),
        };
        if (format, kind) == (mandatory.0, mandatory.1) {
            return Ok(mandatory.2);
        }
        let mut native_format = 0;
        let mut native_kind = 0;
        unsafe {
            gl::GetIntegerv(0x8b9b, &mut native_format);
            gl::GetIntegerv(0x8b9a, &mut native_kind);
        }
        self.driver_result()?;
        if (format, kind) != (native_format as u32, native_kind as u32) {
            return Err(gl::INVALID_OPERATION);
        }
        super::core_texture_formats::read_bytes(format, kind)
    }
}
