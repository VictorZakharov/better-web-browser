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
        self.read_pixels_data(c, input.map(std::borrow::Cow::Borrowed))
    }

    pub(super) fn read_pixels_data(
        &mut self,
        c: &Command,
        input: Option<std::borrow::Cow<'_, [u8]>>,
    ) -> Result<Vec<u8>> {
        let _default_read = self.resolved_default_read()?;
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
        let layout = super::pixel_layout::Store::native(
            self.options.api,
            super::pixel_layout::Direction::Pack,
        )?
        .layout(width as usize, height as usize, 1, bpp, false)?;
        let size = layout.size;
        if size > MAX_UPLOAD_BYTES {
            return Err(gl::OUT_OF_MEMORY);
        }
        if (c.u(6)? as usize) < size {
            return Err(gl::INVALID_OPERATION);
        }
        if input.as_ref().is_some_and(|data| data.len() < size) {
            return Err(gl::INVALID_OPERATION);
        }
        let default_read = if core {
            self.read_framebuffer == 0
        } else {
            self.framebuffer == 0
        };
        let region = super::readback_cache::Region {
            x: c.n(0)?,
            y: c.n(1)?,
            width,
            height,
        };
        // A successful, tight, in-bounds RGBA8 read has no caller-owned
        // padding to preserve. Reuse its owned native pixels while the default
        // buffer is unchanged; arbitrary FBOs and pack routes remain native.
        let reusable = default_read
            && self.default_read_buffer != gl::NONE
            && format == gl::RGBA
            && kind == gl::UNSIGNED_BYTE
            && layout.start == 0
            && layout.row_stride == width as usize * 4
            && region.x >= 0
            && region.y >= 0
            && i64::from(region.x) + i64::from(width) <= i64::from(self.surface.width)
            && i64::from(region.y) + i64::from(height) <= i64::from(self.surface.height);
        if reusable && let Some(bytes) = self.readback_cache.read(region) {
            return Ok(bytes);
        }
        // Preserve destination padding and out-of-bounds pixels, just like the
        // byte path. The native driver receives only our bounded owned storage.
        let mut bytes = match input {
            Some(data) => {
                let mut data = data.into_owned();
                data.truncate(size);
                // Exact-sized V8 copies move unchanged. Do not return excess
                // retained capacity or bytes beyond the admitted layout.
                if data.capacity() == size {
                    data
                } else {
                    data.into_boxed_slice().into_vec()
                }
            }
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
        if default_read && !self.options.alpha && format == gl::RGBA && kind == gl::UNSIGNED_BYTE {
            let stride = layout.row_stride;
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
                        bytes[layout.start + row * stride + column * 4 + 3] = 255;
                    }
                }
            }
        }
        if reusable {
            self.resource_bytes += self.readback_cache.record(
                region,
                &bytes,
                self.resource_limit.saturating_sub(self.resource_bytes),
            );
        }
        Ok(bytes)
    }
}

#[cfg(test)]
mod owned_tests;
