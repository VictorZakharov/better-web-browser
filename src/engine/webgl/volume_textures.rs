//! Bounded GLES3 volume/array storage and owned CPU upload overloads.
use super::core_texture_formats as formats;
use super::core_textures::Image;
use super::texture_targets::{ARRAY, VOLUME};
use super::{ApiVersion, Command, Kind, MAX_UPLOAD_BYTES, Result, WebGl, gl};
use serde_json::Value;

impl WebGl {
    pub(super) fn volume_texture_command(
        &mut self,
        c: &Command,
        bytes: Option<&[u8]>,
    ) -> Result<Value> {
        if self.options.api != ApiVersion::Two {
            return Err(gl::INVALID_OPERATION);
        }
        let target = c.u(0)?;
        if ![VOLUME, ARRAY].contains(&target) {
            return Err(gl::INVALID_ENUM);
        }
        let slot = self.texture_slot(target)?;
        let id = self.textures[self.texture_unit][slot];
        self.objects.get(id, Kind::Texture)?;
        if c.op == "texStorage3D" {
            return self.volume_storage(c, id);
        }
        let from_buffer = c.op.ends_with("FromBuffer");
        if !from_buffer
            && self
                .core_buffer_bindings
                .get(&super::core_buffers::PIXEL_UNPACK)
                .is_some_and(|id| *id != 0)
        {
            return Err(gl::INVALID_OPERATION);
        }
        let sub = c.op.starts_with("texSubImage3D");
        let level = c.n(1)?;
        if !(0..=12).contains(&level) {
            return Err(gl::INVALID_VALUE);
        }
        let first_dimension = if sub { 5 } else { 3 };
        let (width, height, depth) = (
            c.n(first_dimension)?,
            c.n(first_dimension + 1)?,
            c.n(first_dimension + 2)?,
        );
        dimensions(target, width, height, depth)?;
        let (format, kind) = (c.u(if sub { 8 } else { 7 })?, c.u(if sub { 9 } else { 8 })?);
        if kind == 0x8dad && bytes.is_some() && !from_buffer {
            return Err(gl::INVALID_OPERATION);
        }
        let object = self.objects.get(id, Kind::Texture)?;
        let (internal, offsets) = if sub {
            let image = object
                .core_images
                .get(&(target, level))
                .ok_or(gl::INVALID_OPERATION)?;
            let offsets = [c.n(2)?, c.n(3)?, c.n(4)?];
            for ((offset, size), extent) in offsets.into_iter().zip([width, height, depth]).zip([
                image.width,
                image.height,
                image.depth,
            ]) {
                if offset < 0 || offset as u64 + size as u64 > extent as u64 {
                    return Err(gl::INVALID_VALUE);
                }
            }
            if bytes.is_none() && !from_buffer {
                return Err(gl::INVALID_VALUE);
            }
            (image.internal, offsets)
        } else {
            if object.immutable_levels != 0 {
                return Err(gl::INVALID_OPERATION);
            }
            if c.n(6)? != 0 {
                return Err(gl::INVALID_VALUE);
            }
            (c.u(2)?, [0; 3])
        };
        let (upload, storage) = formats::upload(internal, format, kind)?;
        let size = if bytes.is_none() && !sub && !from_buffer {
            0
        } else {
            super::pixel_layout::Store::native(
                self.options.api,
                super::pixel_layout::Direction::Unpack,
            )?
            .layout(
                width as usize,
                height as usize,
                depth as usize,
                upload,
                true,
            )?
            .size
        };
        if size > MAX_UPLOAD_BYTES {
            return Err(gl::OUT_OF_MEMORY);
        }
        if bytes.is_some_and(|data| data.len() < size) {
            return Err(gl::INVALID_OPERATION);
        }
        let allocation = allocation(width as u32, height as u32, depth as u32, storage)?;
        if !sub {
            self.charge(0, allocation)?;
        }
        let pointer =
            self.unpack_pointer(c, bytes, from_buffer, if sub { 10 } else { 9 }, size, kind)?;
        let core = self.core.as_ref().ok_or(gl::INVALID_OPERATION)?;
        // The validated footprint includes every skipped row/image and padding.
        unsafe {
            if sub {
                (core.texture_sub_image_3d)(
                    target, level, offsets[0], offsets[1], offsets[2], width, height, depth,
                    format, kind, pointer,
                );
            } else {
                (core.texture_image_3d)(
                    target,
                    level,
                    internal as i32,
                    width,
                    height,
                    depth,
                    0,
                    format,
                    kind,
                    pointer,
                );
            }
        }
        self.driver_result()?;
        if !sub {
            if bytes.is_none() && !from_buffer {
                self.initialize_volume(
                    target,
                    level,
                    Image {
                        internal,
                        width: width as u32,
                        height: height as u32,
                        depth: depth as u32,
                    },
                )?;
            }
            let object = self.objects.get_mut(id, Kind::Texture)?;
            object.capacity = object.capacity.max(allocation);
            object.core_images.insert(
                (target, level),
                Image {
                    internal,
                    width: width as u32,
                    height: height as u32,
                    depth: depth as u32,
                },
            );
            object
                .texture_images
                .insert((target, level), (format, kind));
        }
        Ok(Value::Null)
    }

    fn volume_storage(&mut self, c: &Command, id: u32) -> Result<Value> {
        if super::compressed_formats::format(c.u(2)?).is_ok() {
            return self.compressed_texture_storage(c);
        }
        let target = c.u(0)?;
        if self.objects.get(id, Kind::Texture)?.immutable_levels != 0 {
            return Err(gl::INVALID_OPERATION);
        }
        let levels = c.n(1)?;
        let internal = c.u(2)?;
        let storage = formats::storage(internal)?;
        let (width, height, depth) = (c.n(3)?, c.n(4)?, c.n(5)?);
        dimensions(target, width, height, depth)?;
        if levels < 1 || width < 1 || height < 1 || depth < 1 {
            return Err(gl::INVALID_VALUE);
        }
        let largest = if target == VOLUME {
            width.max(height).max(depth)
        } else {
            width.max(height)
        };
        if levels as u32 > 32 - (largest as u32).leading_zeros() {
            return Err(gl::INVALID_OPERATION);
        }
        let mut images = Vec::new();
        let mut total = 0usize;
        for level in 0..levels {
            let image = Image {
                internal,
                width: (width as u32 >> level).max(1),
                height: (height as u32 >> level).max(1),
                depth: if target == VOLUME {
                    (depth as u32 >> level).max(1)
                } else {
                    depth as u32
                },
            };
            total = total
                .checked_add(allocation(
                    image.width,
                    image.height,
                    image.depth,
                    storage.bytes,
                )?)
                .ok_or(gl::OUT_OF_MEMORY)?;
            images.push(((target, level), image));
        }
        self.charge(0, total)?;
        let function = self
            .core
            .as_ref()
            .ok_or(gl::INVALID_OPERATION)?
            .texture_storage_3d;
        unsafe {
            function(target, levels, internal, width, height, depth);
        }
        self.driver_result()?;
        for ((_, level), image) in &images {
            self.initialize_volume(target, *level, *image)?;
        }
        let object = self.objects.get_mut(id, Kind::Texture)?;
        object.capacity = total;
        object.immutable_levels = levels as u32;
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
}

fn dimensions(target: u32, width: i32, height: i32, depth: i32) -> Result<()> {
    let max_dimension = 4096;
    let max_depth = if target == VOLUME { 4096 } else { 256 };
    if !(0..=max_dimension).contains(&width)
        || !(0..=max_dimension).contains(&height)
        || !(0..=max_depth).contains(&depth)
    {
        return Err(gl::INVALID_VALUE);
    }
    Ok(())
}
fn allocation(width: u32, height: u32, depth: u32, bytes: usize) -> Result<usize> {
    (width as usize)
        .checked_mul(height as usize)
        .and_then(|n| n.checked_mul(depth as usize))
        .and_then(|n| n.checked_mul(bytes))
        .ok_or(gl::OUT_OF_MEMORY)
}
