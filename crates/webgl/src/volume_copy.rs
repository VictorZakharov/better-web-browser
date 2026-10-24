//! GLES3 framebuffer copies update one existing volume/array slice.
//! https://registry.khronos.org/webgl/specs/latest/2.0/#3.7.6
use super::texture_targets::{ARRAY, VOLUME};
use super::{ApiVersion, Command, Kind, Result, WebGl, gl};
use serde_json::Value;

impl WebGl {
    pub(super) fn copy_volume_texture(&mut self, c: &Command) -> Result<Value> {
        if self.options.api != ApiVersion::Two {
            return Err(gl::INVALID_OPERATION);
        }
        let target = c.u(0)?;
        if !matches!(target, ARRAY | VOLUME) {
            return Err(gl::INVALID_ENUM);
        }
        let level = c.n(1)?;
        if !(0..=12).contains(&level) {
            return Err(gl::INVALID_VALUE);
        }
        let slot = self.texture_slot(target)?;
        let id = self.textures[self.texture_unit][slot];
        let object = self.objects.get(id, Kind::Texture)?;
        let image = *object
            .core_images
            .get(&(target, level))
            .ok_or(gl::INVALID_OPERATION)?;
        let (xoffset, yoffset, zoffset) = (c.n(2)?, c.n(3)?, c.n(4)?);
        let (x, y, width, height) = (c.n(5)?, c.n(6)?, c.n(7)?, c.n(8)?);
        if xoffset < 0
            || yoffset < 0
            || zoffset < 0
            || width < 0
            || height < 0
            || xoffset as u64 + width as u64 > image.width as u64
            || yoffset as u64 + height as u64 > image.height as u64
            || zoffset as u32 >= image.depth
        {
            return Err(gl::INVALID_VALUE);
        }
        if super::compressed_formats::format(image.internal).is_ok() {
            return Err(gl::INVALID_OPERATION);
        }
        self.validate_read_framebuffer()?;
        let function = self
            .core
            .as_ref()
            .ok_or(gl::INVALID_OPERATION)?
            .copy_texture_sub_image_3d;
        if target == VOLUME {
            x.checked_add(width).ok_or(gl::INVALID_VALUE)?;
            y.checked_add(height).ok_or(gl::INVALID_VALUE)?;
            // Reuse ANGLE's complete CopyTexSubImage format/read-route/MSAA/
            // feedback validation without entering its corrupting staging path.
            // The pinned ValidateES3CopyTexImageParametersBase checks all those
            // contracts BEFORE returning false for zero extent, without an error
            // or a driver copy. Browser bounds above validate the real extent.
            unsafe {
                function(target, level, xoffset, yoffset, zoffset, x, y, 0, 0);
            }
            self.driver_result()?;
            if width != 0 && height != 0 {
                self.copy_framebuffer_to_volume(
                    id,
                    level,
                    [xoffset, yoffset, zoffset],
                    [x, y, width, height],
                )?;
            }
            return Ok(Value::Null);
        }
        // Read-buffer selection, integer/normalized format compatibility,
        // multisample prohibition and feedback loops remain ANGLE validation.
        // This is a GPU copy: author pack/unpack state and PBOs are irrelevant.
        unsafe {
            function(
                target, level, xoffset, yoffset, zoffset, x, y, width, height,
            );
        }
        self.driver_result()?;
        Ok(Value::Null)
    }
}
