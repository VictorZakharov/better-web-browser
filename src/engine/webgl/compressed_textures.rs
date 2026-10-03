//! Bounded owned block uploads reuse ANGLE's decoder/native BC storage.
use super::{Command, Kind, MAX_UPLOAD_BYTES, Result, WebGl, gl};
use serde_json::Value;
impl WebGl {
    pub(super) fn compressed_texture_command(
        &mut self,
        c: &Command,
        bytes: Option<&[u8]>,
    ) -> Result<Value> {
        let sub = c.op == "compressedTexSubImage2D";
        let target = c.u(0)?;
        let cube =
            (gl::TEXTURE_CUBE_MAP_POSITIVE_X..=gl::TEXTURE_CUBE_MAP_NEGATIVE_Z).contains(&target);
        let slot = if cube {
            1
        } else if target == gl::TEXTURE_2D {
            0
        } else {
            return Err(gl::INVALID_ENUM);
        };
        let level = c.n(1)?;
        if !(0..=12).contains(&level) {
            return Err(gl::INVALID_VALUE);
        }
        let (width, height) = (c.n(if sub { 4 } else { 3 })?, c.n(if sub { 5 } else { 4 })?);
        if width < 0 || height < 0 || width > (4096 >> level) || height > (4096 >> level) {
            return Err(gl::INVALID_VALUE);
        }
        let internal = c.u(if sub { 6 } else { 2 })?;
        let format = super::compressed_formats::format(internal)?;
        if !self.extensions.compressed.enabled(format.family) {
            return Err(gl::INVALID_ENUM);
        }
        let bytes = bytes.ok_or(gl::INVALID_VALUE)?;
        let expected = format.byte_size(width as u32, height as u32)?;
        if expected > MAX_UPLOAD_BYTES {
            return Err(gl::INVALID_VALUE);
        }
        // CPU overloads must not let an owned pointer be interpreted as an
        // offset into an author's unpack buffer, even for a zero-byte image.
        if self.options.api == super::ApiVersion::Two
            && *self
                .core_buffer_bindings
                .get(&super::core_buffers::PIXEL_UNPACK)
                .unwrap_or(&0)
                != 0
        {
            return Err(gl::INVALID_OPERATION);
        }
        let id = self.textures[self.texture_unit][slot];
        let object = self.objects.get(id, Kind::Texture)?;
        if sub {
            let image = object
                .core_images
                .get(&(target, level))
                .ok_or(gl::INVALID_OPERATION)?;
            if image.internal != internal {
                return Err(gl::INVALID_OPERATION);
            }
            // A mismatched allocated format is INVALID_OPERATION even when
            // the same payload has a different footprint in the new format.
            if bytes.len() != expected {
                return Err(gl::INVALID_VALUE);
            }
            let (x, y) = (c.n(2)?, c.n(3)?);
            if x < 0 || y < 0 {
                return Err(gl::INVALID_VALUE);
            }
            format.subregion(
                (image.width, image.height),
                (x as u32, y as u32),
                (width as u32, height as u32),
            )?;
            unsafe {
                gl::CompressedTexSubImage2D(
                    target,
                    level,
                    x,
                    y,
                    width,
                    height,
                    internal,
                    expected as i32,
                    bytes.as_ptr().cast(),
                )
            };
        } else {
            if c.n(5)? != 0 || cube && width != height {
                return Err(gl::INVALID_VALUE);
            }
            if object.immutable_levels != 0 {
                return Err(gl::INVALID_OPERATION);
            }
            format.image_dimensions(width as u32, height as u32, level as u32)?;
            if bytes.len() != expected {
                return Err(gl::INVALID_VALUE);
            }
            // Charge against uncompressed residency, not just tiny transport
            // blocks: a provider may decompress before storing the texture.
            let size = (width as usize)
                .checked_mul(height as usize)
                .and_then(|size| size.checked_mul(4))
                .ok_or(gl::OUT_OF_MEMORY)?;
            let previous = object.capacity;
            let old_size = object
                .core_images
                .get(&(target, level))
                .map_or(0, |image| image.width as usize * image.height as usize * 4);
            let capacity = previous
                .checked_add(size.saturating_sub(old_size))
                .ok_or(gl::OUT_OF_MEMORY)?;
            self.charge(previous, capacity)?;
            unsafe {
                gl::CompressedTexImage2D(
                    target,
                    level,
                    internal,
                    width,
                    height,
                    0,
                    expected as i32,
                    bytes.as_ptr().cast(),
                )
            };
            self.driver_result()?;
            let object = self.objects.get_mut(id, Kind::Texture)?;
            object.capacity = capacity;
            object
                .texture_images
                .insert((target, level), (internal, gl::UNSIGNED_BYTE));
            object.core_images.insert(
                (target, level),
                super::core_textures::Image {
                    internal,
                    width: width as u32,
                    height: height as u32,
                    depth: 1,
                },
            );
        }
        self.driver_result()?;
        Ok(Value::Null)
    }
}
