//! Copy paths have the same dimension/resource bounds as explicit texture uploads.
use super::{Command, Kind, Result, WebGl, gl};
use serde_json::Value;
impl WebGl {
    pub(super) fn copy_texture(&mut self, c: &Command) -> Result<Value> {
        if self.options.api == super::ApiVersion::Two {
            return self.copy_core_texture(c);
        }
        let target = c.u(0)?;
        let slot = if target == gl::TEXTURE_2D {
            0
        } else if (gl::TEXTURE_CUBE_MAP_POSITIVE_X..=gl::TEXTURE_CUBE_MAP_NEGATIVE_Z)
            .contains(&target)
        {
            1
        } else {
            return Err(gl::INVALID_ENUM);
        };
        let id = self.textures[self.texture_unit][slot];
        self.objects.get(id, Kind::Texture)?;
        let level = c.n(1)?;
        let sub = c.op == "copyTexSubImage2D";
        let mut reservation = None;
        if sub
            && self
                .objects
                .get(id, Kind::Texture)?
                .texture_images
                .get(&(target, level))
                .is_some_and(|(format, _)| super::depth_textures::is_depth(*format))
        {
            return Err(gl::INVALID_OPERATION);
        }
        self.validate_framebuffer()?;
        let width = c.n(if sub { 6 } else { 5 })?;
        let height = c.n(if sub { 7 } else { 6 })?;
        if !(0..=12).contains(&level)
            || !(0..=4096).contains(&width)
            || !(0..=4096).contains(&height)
        {
            return Err(gl::INVALID_VALUE);
        }
        if !sub {
            let format = c.u(2)?;
            if super::depth_textures::is_depth(format)
                && self
                    .extensions
                    .textures
                    .enabled(super::texture_capabilities::TextureCapability::Depth)
            {
                return Err(gl::INVALID_OPERATION);
            }
            if ![
                gl::ALPHA,
                gl::RGB,
                gl::RGBA,
                gl::LUMINANCE,
                gl::LUMINANCE_ALPHA,
            ]
            .contains(&format)
            {
                return Err(gl::INVALID_ENUM);
            }
            if c.n(7)? != 0 || (slot == 1 && width != height) {
                return Err(gl::INVALID_VALUE);
            }
            reservation = Some(self.prepare_texture_storage(
                id,
                vec![((target, level), width as usize * height as usize * 4)],
            )?);
            if self.color_read_type()? == gl::FLOAT {
                self.copy_float_to_normalized(
                    target,
                    level,
                    format,
                    [c.n(3)?, c.n(4)?, width, height],
                    None,
                )?;
            } else {
                unsafe {
                    gl::CopyTexImage2D(target, level, format, c.n(3)?, c.n(4)?, width, height, 0);
                }
            }
        } else {
            let (format, kind) = *self
                .objects
                .get(id, Kind::Texture)?
                .texture_images
                .get(&(target, level))
                .ok_or(gl::INVALID_OPERATION)?;
            let xoffset = c.n(2)?;
            let yoffset = c.n(3)?;
            if xoffset < 0 || yoffset < 0 {
                return Err(gl::INVALID_VALUE);
            }
            if kind == gl::UNSIGNED_BYTE && self.color_read_type()? == gl::FLOAT {
                // EXT_color_buffer_half_float issue 9 retains the GLES2
                // float-to-normalized CopyTex(Sub)Image conversion contract.
                self.copy_float_to_normalized(
                    target,
                    level,
                    format,
                    [c.n(4)?, c.n(5)?, width, height],
                    Some([xoffset, yoffset]),
                )?;
            } else {
                unsafe {
                    gl::CopyTexSubImage2D(
                        target,
                        level,
                        c.n(2)?,
                        c.n(3)?,
                        c.n(4)?,
                        c.n(5)?,
                        width,
                        height,
                    );
                }
            }
        }
        self.driver_result()?;
        if let Some(reservation) = reservation {
            self.commit_texture_storage(reservation)?;
        }
        if !sub {
            self.objects.get_mut(id, Kind::Texture)?.core_images.insert(
                (target, level),
                super::core_textures::Image {
                    internal: c.u(2)?,
                    width: width as u32,
                    height: height as u32,
                    depth: 1,
                },
            );
            self.objects
                .get_mut(id, Kind::Texture)?
                .texture_images
                .insert((target, level), (c.u(2)?, gl::UNSIGNED_BYTE));
        }
        Ok(Value::Null)
    }
}
