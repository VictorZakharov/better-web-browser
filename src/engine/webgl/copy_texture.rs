//! Copy paths have the same dimension/resource bounds as explicit texture uploads.
use super::{Command, Kind, Result, WebGl, gl};
use serde_json::Value;
impl WebGl {
    pub(super) fn copy_texture(&mut self, c: &Command) -> Result<Value> {
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
            self.charge(0, width as usize * height as usize * 4)?;
            self.objects.get_mut(id, Kind::Texture)?.capacity =
                width as usize * height as usize * 4;
            unsafe {
                gl::CopyTexImage2D(target, level, format, c.n(3)?, c.n(4)?, width, height, 0);
            }
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
        self.driver_result()?;
        Ok(Value::Null)
    }
}
