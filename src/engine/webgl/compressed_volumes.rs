//! Checked owned BC array slices and distinct native buffer-offset overloads.
use super::{ApiVersion, Command, Kind, MAX_UPLOAD_BYTES, Result, WebGl, gl};
use serde_json::Value;
impl WebGl {
    pub(super) fn compressed_volume_command(
        &mut self,
        c: &Command,
        bytes: Option<&[u8]>,
    ) -> Result<Value> {
        if self.options.api != ApiVersion::Two {
            return Err(gl::INVALID_OPERATION);
        }
        let target = c.u(0)?;
        if !matches!(target, 0x8c1a | 0x806f) {
            return Err(gl::INVALID_ENUM);
        }
        let sub = c.op.starts_with("compressedTexSub");
        let from_buffer = c.op.ends_with("FromBuffer");
        if !from_buffer
            && self
                .core_buffer_bindings
                .get(&super::core_buffers::PIXEL_UNPACK)
                .is_some_and(|id| *id != 0)
        {
            return Err(gl::INVALID_OPERATION);
        }
        let level = c.n(1)?;
        if !(0..=12).contains(&level) {
            return Err(gl::INVALID_VALUE);
        }
        let first = if sub { 5 } else { 3 };
        let (width, height, depth) = (c.n(first)?, c.n(first + 1)?, c.n(first + 2)?);
        if !(0..=(4096 >> level)).contains(&width)
            || !(0..=(4096 >> level)).contains(&height)
            || !(0..=256).contains(&depth)
        {
            return Err(gl::INVALID_VALUE);
        }
        let internal = c.u(if sub { 8 } else { 2 })?;
        let format = super::compressed_formats::format(internal)?;
        if !self.extensions.compressed.enabled(format.family) {
            return Err(gl::INVALID_ENUM);
        }
        if target == 0x806f {
            return Err(gl::INVALID_OPERATION);
        }
        let expected = format
            .byte_size(width as u32, height as u32)?
            .checked_mul(depth as usize)
            .ok_or(gl::OUT_OF_MEMORY)?;
        if expected > MAX_UPLOAD_BYTES {
            return Err(gl::INVALID_VALUE);
        }
        let id = self.textures[self.texture_unit][3];
        let object = self.objects.get(id, Kind::Texture)?;
        if sub {
            let image = object
                .core_images
                .get(&(target, level))
                .ok_or(gl::INVALID_OPERATION)?;
            if image.internal != internal {
                return Err(gl::INVALID_OPERATION);
            }
            let pointer = self.compressed_pointer(c, bytes, from_buffer, 9, expected)?;
            let (x, y, z) = (c.n(2)?, c.n(3)?, c.n(4)?);
            if x < 0 || y < 0 || z < 0 || z as u64 + depth as u64 > image.depth as u64 {
                return Err(gl::INVALID_VALUE);
            }
            format.subregion(
                (image.width, image.height),
                (x as u32, y as u32),
                (width as u32, height as u32),
            )?;
            let function = self
                .core
                .as_ref()
                .ok_or(gl::INVALID_OPERATION)?
                .compressed_sub_image_3d;
            unsafe {
                function(
                    target,
                    level,
                    x,
                    y,
                    z,
                    width,
                    height,
                    depth,
                    internal,
                    expected as i32,
                    pointer,
                )
            };
        } else {
            if object.immutable_levels != 0 {
                return Err(gl::INVALID_OPERATION);
            }
            if c.n(6)? != 0 {
                return Err(gl::INVALID_VALUE);
            }
            format.image_dimensions(width as u32, height as u32, level as u32)?;
            let pointer = self.compressed_pointer(c, bytes, from_buffer, 7, expected)?;
            let size = width as usize * height as usize * depth as usize * 4;
            let reservation = self.prepare_texture_storage(id, vec![((target, level), size)])?;
            let function = self
                .core
                .as_ref()
                .ok_or(gl::INVALID_OPERATION)?
                .compressed_image_3d;
            unsafe {
                function(
                    target,
                    level,
                    internal,
                    width,
                    height,
                    depth,
                    0,
                    expected as i32,
                    pointer,
                )
            };
            self.driver_result()?;
            self.commit_texture_storage(reservation)?;
            let object = self.objects.get_mut(id, Kind::Texture)?;
            object
                .texture_images
                .insert((target, level), (internal, gl::UNSIGNED_BYTE));
            object.core_images.insert(
                (target, level),
                super::core_textures::Image {
                    internal,
                    width: width as u32,
                    height: height as u32,
                    depth: depth as u32,
                },
            );
        }
        self.driver_result()?;
        Ok(Value::Null)
    }
}
