//! GLES3 copies use the selected read framebuffer and sized destination formats.
//! ANGLE validates source/destination component classes, MSAA and feedback;
//! the browser owns admission bounds, storage accounting and image metadata.
//! https://registry.khronos.org/OpenGL-Refpages/es3.0/html/glCopyTexImage2D.xhtml
use super::{Command, Kind, Result, WebGl, gl};
use serde_json::Value;

impl WebGl {
    pub(super) fn copy_core_texture(&mut self, command: &Command) -> Result<Value> {
        let target = command.u(0)?;
        let slot = match target {
            gl::TEXTURE_2D => 0,
            gl::TEXTURE_CUBE_MAP_POSITIVE_X..=gl::TEXTURE_CUBE_MAP_NEGATIVE_Z => 1,
            _ => return Err(gl::INVALID_ENUM),
        };
        let id = self.textures[self.texture_unit][slot];
        let object = self.objects.get(id, Kind::Texture)?;
        let sub = command.op == "copyTexSubImage2D";
        let level = command.n(1)?;
        let width = command.n(if sub { 6 } else { 5 })?;
        let height = command.n(if sub { 7 } else { 6 })?;
        if !(0..=12).contains(&level)
            || !(0..=4096).contains(&width)
            || !(0..=4096).contains(&height)
        {
            return Err(gl::INVALID_VALUE);
        }
        let mut allocation = None;
        let mut metadata = None;
        let internal;
        if sub {
            let image = object
                .core_images
                .get(&(target, level))
                .ok_or(gl::INVALID_OPERATION)?;
            internal = image.internal;
            let x = command.n(2)?;
            let y = command.n(3)?;
            if x < 0
                || y < 0
                || x as u64 + width as u64 > image.width as u64
                || y as u64 + height as u64 > image.height as u64
            {
                return Err(gl::INVALID_VALUE);
            }
        } else {
            if object.immutable_levels != 0 {
                return Err(gl::INVALID_OPERATION);
            }
            internal = command.u(2)?;
            self.validate_normalized_texture(internal, gl::INVALID_ENUM)?;
            let (base, kind, bytes) = copy_format(internal)?;
            if command.n(7)? != 0 || slot == 1 && width != height {
                return Err(gl::INVALID_VALUE);
            }
            allocation = Some(self.prepare_texture_storage(
                id,
                vec![((target, level), width as usize * height as usize * bytes)],
            )?);
            metadata = Some((internal, base, kind));
        }
        // Copies read READ_FRAMEBUFFER, never the separately bound draw target.
        // Transparent default-buffer resolution is performed by dispatch first.
        self.validate_read_framebuffer()?;
        let x = command.n(if sub { 4 } else { 3 })?;
        let y = command.n(if sub { 5 } else { 4 })?;
        x.checked_add(width).ok_or(gl::INVALID_VALUE)?;
        y.checked_add(height).ok_or(gl::INVALID_VALUE)?;
        let conversion = super::volume_copy_conversion::Transfer::for_format(internal).ok();
        if let Some(plan) = conversion {
            self.validate_core_copy_workspace(
                plan,
                [width, height],
                allocation
                    .as_ref()
                    .map_or(self.resource_bytes, |value| value.counter()),
            )?;
        }
        // SAFETY: browser-owned current bindings; bounded nonnegative extents;
        // no client pointers or author PBO state are consumed by GPU copies.
        unsafe {
            if sub {
                gl::CopyTexSubImage2D(
                    target,
                    level,
                    command.n(2)?,
                    command.n(3)?,
                    x,
                    y,
                    width,
                    height,
                );
            } else {
                gl::CopyTexImage2D(target, level, command.u(2)?, x, y, width, height, 0);
            }
        }
        // Failed native copies must preserve the old image and budget charge.
        self.driver_result()?;
        if let Some(allocation) = allocation {
            self.commit_texture_storage(allocation)?;
        }
        if let Some((internal, base, kind)) = metadata {
            let object = self.objects.get_mut(id, Kind::Texture)?;
            object.core_images.insert(
                (target, level),
                super::core_textures::Image {
                    internal,
                    width: width as u32,
                    height: height as u32,
                    depth: 1,
                },
            );
            object.texture_images.insert((target, level), (base, kind));
        }
        if let Some(plan) = conversion {
            let offsets = if sub {
                [command.n(2)?, command.n(3)?]
            } else {
                [0, 0]
            };
            self.correct_rgb_copy_alpha(target, level, plan, [x, y, width, height], offsets, sub)?;
        }
        Ok(Value::Null)
    }
}

fn copy_format(internal: u32) -> Result<(u32, u32, usize)> {
    if [
        gl::ALPHA,
        gl::LUMINANCE,
        gl::LUMINANCE_ALPHA,
        gl::RGB,
        gl::RGBA,
    ]
    .contains(&internal)
    {
        return Ok((internal, gl::UNSIGNED_BYTE, 4));
    }
    let format = super::core_texture_formats::storage(internal)?;
    // Depth/stencil and other incompatible classes remain native GL errors,
    // not a browser substitution into a normalized RGBA destination.
    Ok((format.base, format.types[0], format.bytes))
}
