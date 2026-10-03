//! Float readback extends the byte transport without exposing driver pointers.
//! Format/type admission follows EXT_color_buffer_half_float's WebGL table.
use super::texture_capabilities::TextureCapability as Capability;
use super::{Command, MAX_UPLOAD_BYTES, Result, WebGl, gl};

impl WebGl {
    pub(super) fn color_read_type(&mut self) -> Result<u32> {
        self.validate_framebuffer()?;
        let color_float = self.extensions.textures.enabled(Capability::ColorFloat)
            || self.extensions.textures.enabled(Capability::ColorHalfFloat);
        if self.framebuffer == 0 || !color_float {
            return Ok(gl::UNSIGNED_BYTE);
        }
        let mut component = 0;
        unsafe {
            gl::GetFramebufferAttachmentParameteriv(
                gl::FRAMEBUFFER,
                gl::COLOR_ATTACHMENT0,
                super::texture_formats::COMPONENT_TYPE,
                &mut component,
            );
        }
        self.driver_result()?;
        Ok(if component as u32 == gl::FLOAT {
            gl::FLOAT
        } else {
            gl::UNSIGNED_BYTE
        })
    }

    pub(super) fn read_pixels(&mut self, c: &Command, input: Option<&[u8]>) -> Result<Vec<u8>> {
        let width = c.n(2)?;
        let height = c.n(3)?;
        if width < 0 || height < 0 {
            return Err(gl::INVALID_VALUE);
        }
        let kind = c.u(5)?;
        if c.u(4)? != gl::RGBA || ![gl::UNSIGNED_BYTE, gl::FLOAT].contains(&kind) {
            return Err(gl::INVALID_OPERATION);
        }
        let color_float = self.extensions.textures.enabled(Capability::ColorFloat)
            || self.extensions.textures.enabled(Capability::ColorHalfFloat);
        if kind == gl::FLOAT && !color_float {
            return Err(gl::INVALID_OPERATION);
        }
        let floating = self.color_read_type()? == gl::FLOAT;
        if floating != (kind == gl::FLOAT) {
            return Err(gl::INVALID_OPERATION);
        }
        let bpp = if floating { 16 } else { 4 };
        let mut alignment = 0;
        unsafe {
            gl::GetIntegerv(gl::PACK_ALIGNMENT, &mut alignment);
        }
        let size =
            super::textures::pixel_size(width as usize, height as usize, bpp, alignment as usize)?;
        if size > MAX_UPLOAD_BYTES {
            return Err(gl::OUT_OF_MEMORY);
        }
        if (c.u(6)? as usize) < size {
            return Err(gl::INVALID_OPERATION);
        }
        // Preserve destination padding and out-of-bounds pixels, just like the
        // byte path. The native driver receives only our bounded owned storage.
        let mut bytes = match input {
            Some(data) if data.len() >= size => data[..size].to_vec(),
            Some(_) => return Err(gl::INVALID_OPERATION),
            None => vec![0u8; size],
        };
        unsafe {
            gl::ReadPixels(
                c.n(0)?,
                c.n(1)?,
                width,
                height,
                gl::RGBA,
                kind,
                bytes.as_mut_ptr().cast(),
            );
        }
        self.driver_result()?;
        if self.framebuffer == 0 && !self.options.alpha {
            let stride = (width as usize * 4).div_ceil(alignment as usize) * alignment as usize;
            let x = c.n(0)?;
            let y = c.n(1)?;
            for row in 0..height as usize {
                for column in 0..width as usize {
                    let sx = i64::from(x) + column as i64;
                    let sy = i64::from(y) + row as i64;
                    if sx >= 0
                        && sy >= 0
                        && sx < i64::from(self.surface.width)
                        && sy < i64::from(self.surface.height)
                    {
                        bytes[row * stride + column * 4 + 3] = 255;
                    }
                }
            }
        }
        Ok(bytes)
    }
}
