//! Bounded texture uploads: no author pointer is ever passed to ANGLE.
use super::texture_capabilities::TextureCapability;
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
                if id == 0 {
                    return Ok(Value::Null);
                }
                for (index, unit) in self.textures.iter_mut().enumerate() {
                    for (slot, binding) in unit.iter_mut().enumerate() {
                        if *binding == id {
                            *binding = 0;
                            unsafe {
                                gl::ActiveTexture(gl::TEXTURE0 + index as u32);
                                gl::BindTexture(super::texture_targets::target(slot), 0);
                            }
                        }
                    }
                }
                unsafe {
                    gl::ActiveTexture(gl::TEXTURE0 + self.texture_unit as u32);
                }
                self.detach_current_resource(id, Kind::Texture)?;
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
                let slot = self.texture_slot(target)?;
                let id = c.u(1)?;
                if id != 0 {
                    let object = self.objects.get_mut(id, Kind::Texture)?;
                    if object.pending_delete {
                        return Err(gl::INVALID_OPERATION);
                    }
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
                self.texture_slot(target)?;
                let pname = c.u(1)?;
                if !self.texture_parameter_allowed(pname) {
                    return Err(gl::INVALID_ENUM);
                }
                if pname == 0x84fe {
                    let value = if c.op == "texParameterf" {
                        c.float(0)?
                    } else {
                        c.n(2)? as f32
                    };
                    let mut maximum = 0.0;
                    unsafe {
                        gl::GetFloatv(0x84ff, &mut maximum);
                    }
                    self.driver_result()?;
                    // EXT permits implementation clamping. ANGLE's WebGL mode
                    // rejects values above its limit, so clamp at our boundary
                    // without changing the specified error for values below 1.
                    let value = if value > maximum { maximum } else { value };
                    unsafe {
                        gl::TexParameterf(target, pname, value);
                    }
                    self.driver_result()?;
                    return Ok(Value::Null);
                }
                unsafe {
                    if c.op == "texParameterf" {
                        gl::TexParameterf(target, pname, c.float(0)?);
                    } else {
                        gl::TexParameteri(target, pname, c.n(2)?);
                    }
                }
            }
            "getTexParameter" => {
                let target = c.u(0)?;
                self.texture_slot(target)?;
                let pname = c.u(1)?;
                if self.options.api == super::ApiVersion::Two && [0x912f, 0x82df].contains(&pname) {
                    let mut value = 0;
                    unsafe {
                        gl::GetTexParameteriv(target, pname, &mut value);
                    }
                    self.driver_result()?;
                    return Ok(if pname == 0x912f {
                        json!(value != 0)
                    } else {
                        json!(value)
                    });
                }
                if self.options.api == super::ApiVersion::Two && [0x813a, 0x813b].contains(&pname) {
                    let mut value = 0.0;
                    unsafe {
                        gl::GetTexParameterfv(target, pname, &mut value);
                    }
                    self.driver_result()?;
                    return Ok(json!(value));
                }
                if !self.texture_parameter_allowed(pname) {
                    return Err(gl::INVALID_ENUM);
                }
                if pname == 0x84fe {
                    let mut value = 0.0;
                    unsafe {
                        gl::GetTexParameterfv(target, pname, &mut value);
                    }
                    self.driver_result()?;
                    return Ok(json!(value));
                }
                let mut value = 0;
                unsafe {
                    gl::GetTexParameteriv(target, pname, &mut value);
                }
                self.driver_result()?;
                return Ok(json!(value));
            }
            "generateMipmap" => {
                if self.options.api == super::ApiVersion::Two {
                    return self.core_generate_mipmap(c);
                }
                let target = c.u(0)?;
                let slot = texture_slot(target)?;
                let id = self.textures[self.texture_unit][slot];
                if self
                    .objects
                    .get(id, Kind::Texture)?
                    .texture_images
                    .iter()
                    .any(|((_, level), (format, _))| {
                        *level == 0
                            && (super::depth_textures::is_depth(*format)
                                || super::texture_color_space::is_srgb(*format)
                                || super::compressed_formats::format(*format).is_ok())
                    })
                {
                    return Err(gl::INVALID_OPERATION);
                }
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
                if super::pixel_layout::extended_parameter(pname) {
                    if self.options.api != super::ApiVersion::Two {
                        return Err(gl::INVALID_ENUM);
                    }
                    let value = c.n(1)?;
                    if value < 0 {
                        return Err(gl::INVALID_VALUE);
                    }
                    unsafe { gl::PixelStorei(pname, value) };
                    self.driver_result()?;
                    return Ok(Value::Null);
                }
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
    fn texture_parameter_allowed(&self, pname: u32) -> bool {
        if self.options.api == super::ApiVersion::Two
            && [0x8072, 0x813a, 0x813b, 0x813c, 0x813d, 0x884c, 0x884d].contains(&pname)
        {
            return true;
        }
        [
            gl::TEXTURE_MIN_FILTER,
            gl::TEXTURE_MAG_FILTER,
            gl::TEXTURE_WRAP_S,
            gl::TEXTURE_WRAP_T,
        ]
        .contains(&pname)
            || pname == 0x84fe
                && self
                    .extensions
                    .textures
                    .enabled(TextureCapability::Anisotropy)
    }
    fn upload_texture(&mut self, c: &Command, bytes: Option<&[u8]>) -> Result<Value> {
        if self.options.api == super::ApiVersion::Two {
            return self.core_texture_upload(c, bytes);
        }
        if self.options.api == super::ApiVersion::Two
            && self
                .core_buffer_bindings
                .get(&super::core_buffers::PIXEL_UNPACK)
                .is_some_and(|id| *id != 0)
        {
            // This opcode carries owned CPU bytes, never a PBO-offset overload.
            return Err(gl::INVALID_OPERATION);
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
        let pixel =
            super::texture_formats::texture_format(format, kind, &self.extensions.textures)?;
        super::depth_textures::validate_upload(c, bytes)?;
        let mut alignment = 0;
        unsafe {
            gl::GetIntegerv(gl::UNPACK_ALIGNMENT, &mut alignment);
        }
        let size = pixel_size(
            width as usize,
            height as usize,
            pixel.upload_bytes,
            alignment as usize,
        )?;
        if size > MAX_UPLOAD_BYTES {
            return Err(gl::OUT_OF_MEMORY);
        }
        if bytes.is_some_and(|b| b.len() < size) {
            return Err(gl::INVALID_OPERATION);
        }
        let sub = c.op == "texSubImage2D";
        if sub
            && self
                .objects
                .get(id, Kind::Texture)?
                .texture_images
                .get(&(target, level))
                != Some(&(format, kind))
        {
            // GLES3 permits conversions that WebGL1's same-format/type upload
            // contract does not. Preserve the public image definition boundary.
            return Err(gl::INVALID_OPERATION);
        }
        if sub && bytes.is_none() {
            return Err(gl::INVALID_VALUE);
        }
        if !sub {
            if c.n(5)? != 0 {
                return Err(gl::INVALID_VALUE);
            }
            if c.u(2)? != format {
                return Err(gl::INVALID_OPERATION);
            }
            // Charge every new level allocation; this deliberately overestimates replacement.
            let storage = (width as usize * height as usize * pixel.storage_bytes).max(size);
            self.charge(0, storage)?;
            let object = self.objects.get_mut(id, Kind::Texture)?;
            object.capacity = object.capacity.max(storage);
        }
        let pointer = bytes.map_or(ptr::null(), |b| b.as_ptr().cast());
        // Preserve GLES2 extension tokens; floating RGBA32F allocation uses
        // ANGLE's sized color-buffer extension internal format.
        let (native_internal, native_format, native_kind) =
            super::texture_formats::native_format(format, kind);
        unsafe {
            if sub {
                gl::TexSubImage2D(
                    target,
                    level,
                    c.n(2)?,
                    c.n(5)?,
                    width,
                    height,
                    native_format,
                    native_kind,
                    pointer,
                );
            } else {
                gl::TexImage2D(
                    target,
                    level,
                    native_internal as i32,
                    width,
                    height,
                    0,
                    native_format,
                    native_kind,
                    pointer,
                );
            }
        }
        self.driver_result()?;
        if !sub {
            self.objects
                .get_mut(id, Kind::Texture)?
                .texture_images
                .insert((target, level), (format, kind));
        }
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
