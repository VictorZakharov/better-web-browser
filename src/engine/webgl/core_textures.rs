//! GLES3 image storage keeps sized formats and immutable extents browser-owned.
use super::core_texture_formats as formats;
use super::{ApiVersion, Command, Kind, MAX_UPLOAD_BYTES, Result, WebGl, gl};
use serde_json::Value;

#[derive(Clone, Copy)]
pub(super) struct Image {
    pub internal: u32,
    pub width: u32,
    pub height: u32,
    pub depth: u32,
}

impl WebGl {
    pub(super) fn core_texture_storage(&mut self, c: &Command) -> Result<Value> {
        if self.options.api != ApiVersion::Two {
            return Err(gl::INVALID_OPERATION);
        }
        let target = c.u(0)?;
        let slot = super::textures::texture_slot(target)?;
        let id = self.textures[self.texture_unit][slot];
        let object = self.objects.get(id, Kind::Texture)?;
        if object.immutable_levels != 0 {
            return Err(gl::INVALID_OPERATION);
        }
        let levels = c.n(1)?;
        let internal = c.u(2)?;
        let storage = formats::storage(internal)?;
        let width = c.n(3)?;
        let height = c.n(4)?;
        if !(1..=4096).contains(&width)
            || !(1..=4096).contains(&height)
            || slot == 1 && width != height
        {
            return Err(gl::INVALID_VALUE);
        }
        let maximum_levels = 32 - (width.max(height) as u32).leading_zeros();
        if levels < 1 {
            return Err(gl::INVALID_VALUE);
        }
        if levels as u32 > maximum_levels {
            return Err(gl::INVALID_OPERATION);
        }
        let mut images = Vec::new();
        let mut bytes = 0usize;
        for level in 0..levels {
            let w = (width as u32 >> level).max(1);
            let h = (height as u32 >> level).max(1);
            let faces: &[u32] = if slot == 1 {
                &[0x8515, 0x8516, 0x8517, 0x8518, 0x8519, 0x851a]
            } else {
                &[gl::TEXTURE_2D]
            };
            for &face in faces {
                bytes = bytes
                    .checked_add(w as usize * h as usize * storage.bytes)
                    .ok_or(gl::OUT_OF_MEMORY)?;
                images.push((
                    (face, level),
                    Image {
                        internal,
                        width: w,
                        height: h,
                        depth: 1,
                    },
                ));
            }
        }
        self.charge(0, bytes)?;
        let function = self
            .core
            .as_ref()
            .ok_or(gl::INVALID_OPERATION)?
            .texture_storage_2d;
        // No pointer input. The native WebGL-compatible provider initializes all levels.
        unsafe {
            function(target, levels, internal, width, height);
        }
        self.driver_result()?;
        let object = self.objects.get_mut(id, Kind::Texture)?;
        object.immutable_levels = levels as u32;
        object.capacity = bytes;
        object.core_images.clear();
        object.texture_images.clear();
        for (key, image) in images {
            object.core_images.insert(key, image);
            object
                .texture_images
                .insert(key, (storage.base, storage.types[0]));
        }
        Ok(Value::Null)
    }

    pub(super) fn core_texture_upload(
        &mut self,
        c: &Command,
        bytes: Option<&[u8]>,
    ) -> Result<Value> {
        if self
            .core_buffer_bindings
            .get(&super::core_buffers::PIXEL_UNPACK)
            .is_some_and(|id| *id != 0)
        {
            return Err(gl::INVALID_OPERATION);
        }
        let target = c.u(0)?;
        let slot = if target == gl::TEXTURE_2D {
            0
        } else if (0x8515..=0x851a).contains(&target) {
            1
        } else {
            return Err(gl::INVALID_ENUM);
        };
        let id = self.textures[self.texture_unit][slot];
        let object = self.objects.get(id, Kind::Texture)?;
        let level = c.n(1)?;
        let width = c.n(3)?;
        let height = c.n(4)?;
        if !(0..=12).contains(&level)
            || !(0..=4096).contains(&width)
            || !(0..=4096).contains(&height)
        {
            return Err(gl::INVALID_VALUE);
        }
        let sub = c.op == "texSubImage2D";
        let internal = if sub {
            let image = object
                .core_images
                .get(&(target, level))
                .ok_or(gl::INVALID_OPERATION)?;
            let x = c.n(2)?;
            let y = c.n(5)?;
            if x < 0
                || y < 0
                || x as u64 + width as u64 > image.width as u64
                || y as u64 + height as u64 > image.height as u64
            {
                return Err(gl::INVALID_VALUE);
            }
            if bytes.is_none() {
                return Err(gl::INVALID_VALUE);
            }
            image.internal
        } else {
            if object.immutable_levels != 0 {
                return Err(gl::INVALID_OPERATION);
            }
            if c.n(5)? != 0 || slot == 1 && width != height {
                return Err(gl::INVALID_VALUE);
            }
            c.u(2)?
        };
        let format = c.u(6)?;
        let kind = c.u(7)?;
        if kind == 0x8dad && bytes.is_some() {
            // WebGL2 permits FLOAT_32_UNSIGNED_INT_24_8_REV only with null CPU data.
            return Err(gl::INVALID_OPERATION);
        }
        let (upload, storage) = formats::upload(internal, format, kind)?;
        let mut alignment = 0;
        unsafe {
            gl::GetIntegerv(gl::UNPACK_ALIGNMENT, &mut alignment);
        }
        self.driver_result()?;
        let size = super::textures::pixel_size(
            width as usize,
            height as usize,
            upload,
            alignment as usize,
        )?;
        if size > MAX_UPLOAD_BYTES {
            return Err(gl::OUT_OF_MEMORY);
        }
        if bytes.is_some_and(|data| data.len() < size) {
            return Err(gl::INVALID_OPERATION);
        }
        if !sub {
            self.charge(0, width as usize * height as usize * storage)?;
        }
        let pointer = bytes.map_or(std::ptr::null(), |data| data.as_ptr().cast());
        // The owned byte range includes aligned rows. PBO overloads are separate;
        // only CPU-backed uploads reach this closed command.
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
                    internal as i32,
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
        if !sub {
            let object = self.objects.get_mut(id, Kind::Texture)?;
            object.capacity = object
                .capacity
                .max(width as usize * height as usize * storage);
            object.core_images.insert(
                (target, level),
                Image {
                    internal,
                    width: width as u32,
                    height: height as u32,
                    depth: 1,
                },
            );
            object
                .texture_images
                .insert((target, level), (format, kind));
        }
        Ok(Value::Null)
    }
}
