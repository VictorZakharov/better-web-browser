//! Float readback extends the byte transport without exposing driver pointers.
//! Format/type admission follows EXT_color_buffer_half_float's WebGL table.
use super::texture_capabilities::TextureCapability as Capability;
use super::{Command, MAX_UPLOAD_BYTES, Result, WebGl, gl};

impl WebGl {
    pub(super) fn color_read_type(&mut self) -> Result<u32> {
        if self.options.api == super::ApiVersion::Two {
            self.validate_read_framebuffer()?;
            return Ok(gl::UNSIGNED_BYTE);
        }
        self.validate_framebuffer()?;
        let color_float = self.extensions.textures.enabled(Capability::ColorFloat)
            || self.extensions.textures.enabled(Capability::ColorHalfFloat);
        if self.framebuffer == 0 || !color_float {
            return Ok(gl::UNSIGNED_BYTE);
        }
        let entries = &self
            .objects
            .get(self.framebuffer, super::Kind::Framebuffer)?
            .framebuffer_attachments;
        let floating = if let Some(entry) = entries.get(&gl::COLOR_ATTACHMENT0) {
            let object = self.objects.get(entry.id, entry.kind)?;
            if entry.kind == super::Kind::Texture {
                object
                    .texture_images
                    .get(&(entry.target, 0))
                    .is_some_and(|(_, kind)| {
                        [gl::FLOAT, super::texture_formats::HALF_FLOAT].contains(kind)
                    })
            } else {
                [
                    super::texture_formats::RGBA32F,
                    super::texture_formats::RGBA16F,
                    super::texture_formats::RGB16F,
                ]
                .contains(&object.renderbuffer_format)
            }
        } else {
            false
        };
        Ok(if floating {
            gl::FLOAT
        } else {
            gl::UNSIGNED_BYTE
        })
    }

    pub(super) fn read_pixels(&mut self, c: &Command, input: Option<&[u8]>) -> Result<Vec<u8>> {
        if self.options.api == super::ApiVersion::Two
            && self
                .core_buffer_bindings
                .get(&super::core_buffers::PIXEL_PACK)
                .is_some_and(|id| *id != 0)
        {
            // A client-memory read and a pixel-pack-buffer offset are distinct overloads.
            return Err(gl::INVALID_OPERATION);
        }
        let width = c.n(2)?;
        let height = c.n(3)?;
        if width < 0 || height < 0 {
            return Err(gl::INVALID_VALUE);
        }
        let kind = c.u(5)?;
        let format = c.u(4)?;
        let core = self.options.api == super::ApiVersion::Two;
        if !core && (format != gl::RGBA || ![gl::UNSIGNED_BYTE, gl::FLOAT].contains(&kind)) {
            return Err(gl::INVALID_OPERATION);
        }
        let color_float = self.extensions.textures.enabled(Capability::ColorFloat)
            || self.extensions.textures.enabled(Capability::ColorHalfFloat);
        if !core && kind == gl::FLOAT && !color_float {
            return Err(gl::INVALID_OPERATION);
        }
        let floating = !core && self.color_read_type()? == gl::FLOAT;
        if !core && floating != (kind == gl::FLOAT) {
            return Err(gl::INVALID_OPERATION);
        }
        let bpp = if core {
            self.core_read_pair(format, kind)?
        } else if floating {
            16
        } else {
            4
        };
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
                format,
                kind,
                bytes.as_mut_ptr().cast(),
            );
        }
        self.driver_result()?;
        let default_read = if self.options.api == super::ApiVersion::Two {
            self.read_framebuffer == 0
        } else {
            self.framebuffer == 0
        };
        if default_read && !self.options.alpha && format == gl::RGBA && kind == gl::UNSIGNED_BYTE {
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
