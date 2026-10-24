//! Immutable compressed storage shares owned mip/layer metadata with sized images.
use super::core_textures::Image;
use super::{ApiVersion, Command, Kind, Result, WebGl, gl};
use serde_json::Value;
impl WebGl {
    pub(super) fn compressed_texture_storage(&mut self, c: &Command) -> Result<Value> {
        if self.options.api != ApiVersion::Two {
            return Err(gl::INVALID_OPERATION);
        }
        let target = c.u(0)?;
        let array = target == 0x8c1a;
        let slot = self.texture_slot(target)?;
        let internal = c.u(2)?;
        let format = super::compressed_formats::format(internal)?;
        if !self.extensions.compressed.enabled(format.family) {
            return Err(gl::INVALID_ENUM);
        }
        // BC families are 2D blocks: their GLES extensions allow array slices,
        // not volumetric TEXTURE_3D storage.
        if target == 0x806f {
            return Err(gl::INVALID_OPERATION);
        }
        let id = self.textures[self.texture_unit][slot];
        let object = self.objects.get(id, Kind::Texture)?;
        if object.immutable_levels != 0 {
            return Err(gl::INVALID_OPERATION);
        }
        let (levels, width, height) = (c.n(1)?, c.n(3)?, c.n(4)?);
        let depth = if array { c.n(5)? } else { 1 };
        if levels < 1
            || !(1..=4096).contains(&width)
            || !(1..=4096).contains(&height)
            || !(1..=256).contains(&depth)
            || slot == 1 && width != height
        {
            return Err(gl::INVALID_VALUE);
        }
        if levels as u32 > 32 - (width.max(height) as u32).leading_zeros() {
            return Err(gl::INVALID_OPERATION);
        }
        let faces: &[u32] = if slot == 1 {
            &[0x8515, 0x8516, 0x8517, 0x8518, 0x8519, 0x851a]
        } else {
            &[target]
        };
        let mut images = Vec::new();
        for level in 0..levels {
            let image = Image {
                internal,
                width: (width as u32 >> level).max(1),
                height: (height as u32 >> level).max(1),
                depth: depth as u32,
            };
            for &face in faces {
                images.push(((face, level), image));
            }
        }
        let reservation = self.prepare_texture_storage(
            id,
            images
                .iter()
                .map(|(key, image)| {
                    (
                        *key,
                        image.width as usize * image.height as usize * image.depth as usize * 4,
                    )
                })
                .collect(),
        )?;
        let core = self.core.as_ref().ok_or(gl::INVALID_OPERATION)?;
        unsafe {
            if array {
                (core.texture_storage_3d)(target, levels, internal, width, height, depth);
            } else {
                (core.texture_storage_2d)(target, levels, internal, width, height);
            }
        }
        self.driver_result()?;
        self.commit_texture_storage(reservation)?;
        for ((face, level), image) in &images {
            self.initialize_compressed_image(*face, *level, *image, array)?;
        }
        let object = self.objects.get_mut(id, Kind::Texture)?;
        object.immutable_levels = levels as u32;
        object.core_images.clear();
        object.texture_images.clear();
        for (key, image) in images {
            object.core_images.insert(key, image);
            object
                .texture_images
                .insert(key, (internal, gl::UNSIGNED_BYTE));
        }
        Ok(Value::Null)
    }
}
