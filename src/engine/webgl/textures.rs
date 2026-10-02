//! Bounded texture uploads: no author pointer is ever passed to ANGLE.
use super::{Command, Kind, MAX_UPLOAD_BYTES, Result, WebGl, gl, json};
use serde_json::Value;
use std::ptr;

impl WebGl {
    pub(super) fn texture_command(&mut self, c: &Command, bytes: Option<&[u8]>) -> Result<Value> {
        match c.op.as_str() {
            "createTexture" => {
                let mut native = 0;
                unsafe {
                    gl::GenTextures(1, &mut native);
                }
                self.driver_result()?;
                match self.objects.insert(Kind::Texture, native) {
                    Ok(id) => return Ok(json!(id)),
                    Err(error) => {
                        unsafe {
                            gl::DeleteTextures(1, &native);
                        }
                        return Err(error);
                    }
                }
            }
            "deleteTexture" => {
                let id = c.u(0)?;
                for unit in &mut self.textures {
                    for binding in unit {
                        if *binding == id {
                            *binding = 0;
                        }
                    }
                }
                self.objects.delete(id, Kind::Texture)?;
            }
            "activeTexture" => {
                let unit = c.u(0)?.checked_sub(gl::TEXTURE0).ok_or(gl::INVALID_ENUM)? as usize;
                if unit >= self.textures.len() {
                    return Err(gl::INVALID_ENUM);
                }
                unsafe {
                    gl::ActiveTexture(gl::TEXTURE0 + unit as u32);
                }
                self.driver_result()?;
                self.texture_unit = unit;
            }
            "bindTexture" => {
                let target = c.u(0)?;
                let slot = texture_slot(target)?;
                let id = c.u(1)?;
                if id != 0 {
                    let object = self.objects.get_mut(id, Kind::Texture)?;
                    if object.buffer_target != 0 && object.buffer_target != target {
                        return Err(gl::INVALID_OPERATION);
                    }
                    object.buffer_target = target;
                }
                let native = self.objects.name(id, Kind::Texture)?;
                unsafe {
                    gl::BindTexture(target, native);
                }
                self.driver_result()?;
                self.textures[self.texture_unit][slot] = id;
            }
            "texImage2D" | "texSubImage2D" => return self.upload_texture(c, bytes),
            "texParameteri" | "texParameterf" => {
                let target = c.u(0)?;
                texture_slot(target)?;
                let pname = c.u(1)?;
                if ![
                    gl::TEXTURE_MIN_FILTER,
                    gl::TEXTURE_MAG_FILTER,
                    gl::TEXTURE_WRAP_S,
                    gl::TEXTURE_WRAP_T,
                ]
                .contains(&pname)
                {
                    return Err(gl::INVALID_ENUM);
                }
                unsafe {
                    gl::TexParameteri(target, pname, c.n(2)?);
                }
            }
            "getTexParameter" => {
                let target = c.u(0)?;
                texture_slot(target)?;
                let pname = c.u(1)?;
                if ![
                    gl::TEXTURE_MIN_FILTER,
                    gl::TEXTURE_MAG_FILTER,
                    gl::TEXTURE_WRAP_S,
                    gl::TEXTURE_WRAP_T,
                ]
                .contains(&pname)
                {
                    return Err(gl::INVALID_ENUM);
                }
                let mut value = 0;
                unsafe {
                    gl::GetTexParameteriv(target, pname, &mut value);
                }
                self.driver_result()?;
                return Ok(json!(value));
            }
            "generateMipmap" => {
                let target = c.u(0)?;
                let slot = texture_slot(target)?;
                let id = self.textures[self.texture_unit][slot];
                let capacity = self.objects.get(id, Kind::Texture)?.capacity;
                // Mip levels consume less than half the base storage. Conservative lifetime
                // accounting also bounds repeated regenerations and driver-held references.
                self.charge(0, capacity / 2)?;
                unsafe {
                    gl::GenerateMipmap(target);
                }
            }
            "pixelStorei" => {
                let pname = c.u(0)?;
                if ![gl::PACK_ALIGNMENT, gl::UNPACK_ALIGNMENT].contains(&pname) {
                    return Err(gl::INVALID_ENUM);
                }
                let value = c.n(1)?;
                if ![1, 2, 4, 8].contains(&value) {
                    return Err(gl::INVALID_VALUE);
                }
                unsafe {
                    gl::PixelStorei(pname, value);
                }
            }
            _ => return Err(gl::INVALID_OPERATION),
        }
        self.driver_result()?;
        Ok(Value::Null)
    }
    fn upload_texture(&mut self, c: &Command, bytes: Option<&[u8]>) -> Result<Value> {
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
        let width = c.n(3)?;
        let height = c.n(4)?;
        let format = c.u(6)?;
        let kind = c.u(7)?;
        if !(0..=12).contains(&level)
            || !(0..=4096).contains(&width)
            || !(0..=4096).contains(&height)
        {
            return Err(gl::INVALID_VALUE);
        }
        if slot == 1 && width != height {
            return Err(gl::INVALID_VALUE);
        }
        let components = match format {
            gl::RGBA => 4,
            gl::RGB => 3,
            gl::LUMINANCE_ALPHA => 2,
            gl::ALPHA | gl::LUMINANCE => 1,
            _ => return Err(gl::INVALID_ENUM),
        };
        let bpp = match kind {
            gl::UNSIGNED_BYTE => components,
            gl::UNSIGNED_SHORT_5_6_5 if format == gl::RGB => 2,
            gl::UNSIGNED_SHORT_4_4_4_4 | gl::UNSIGNED_SHORT_5_5_5_1 if format == gl::RGBA => 2,
            _ => return Err(gl::INVALID_OPERATION),
        };
        let mut alignment = 0;
        unsafe {
            gl::GetIntegerv(gl::UNPACK_ALIGNMENT, &mut alignment);
        }
        let size = pixel_size(width as usize, height as usize, bpp, alignment as usize)?;
        if size > MAX_UPLOAD_BYTES {
            return Err(gl::OUT_OF_MEMORY);
        }
        if bytes.is_some_and(|b| b.len() < size) {
            return Err(gl::INVALID_OPERATION);
        }
        let sub = c.op == "texSubImage2D";
        if sub && bytes.is_none() && size != 0 {
            return Err(gl::INVALID_VALUE);
        }
        if !sub {
            if c.u(2)? != format || c.n(5)? != 0 {
                return Err(gl::INVALID_OPERATION);
            }
            // Charge every new level allocation; this deliberately overestimates replacement.
            self.charge(0, (width as usize * height as usize * 4).max(size))?;
            let object = self.objects.get_mut(id, Kind::Texture)?;
            object.capacity = object.capacity.max(width as usize * height as usize * 4);
        }
        let pointer = bytes.map_or(ptr::null(), |b| b.as_ptr().cast());
        unsafe {
            if sub {
                gl::TexSubImage2D(
                    target,
                    level,
                    c.n(2)?,
                    c.n(5)?,
                    width,
                    height,
                    format,
                    kind,
                    pointer,
                );
            } else {
                gl::TexImage2D(
                    target,
                    level,
                    format as i32,
                    width,
                    height,
                    0,
                    format,
                    kind,
                    pointer,
                );
            }
        }
        self.driver_result()?;
        Ok(Value::Null)
    }
}
pub(super) fn texture_slot(target: u32) -> Result<usize> {
    match target {
        gl::TEXTURE_2D => Ok(0),
        gl::TEXTURE_CUBE_MAP => Ok(1),
        _ => Err(gl::INVALID_ENUM),
    }
}
pub(super) fn pixel_size(
    width: usize,
    height: usize,
    bpp: usize,
    alignment: usize,
) -> Result<usize> {
    if width == 0 || height == 0 {
        return Ok(0);
    }
    if ![1, 2, 4, 8].contains(&alignment) {
        return Err(gl::INVALID_VALUE);
    }
    let row = width.checked_mul(bpp).ok_or(gl::OUT_OF_MEMORY)?;
    let stride = row.checked_add(alignment - 1).ok_or(gl::OUT_OF_MEMORY)? / alignment * alignment;
    stride
        .checked_mul(height - 1)
        .and_then(|n| n.checked_add(row))
        .ok_or(gl::OUT_OF_MEMORY)
}
