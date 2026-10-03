//! Generated GLES3 mip definitions share the upload validation metadata.
use super::core_textures::Image;
use super::{Command, Kind, Result, WebGl, gl};
use serde_json::Value;

impl WebGl {
    pub(super) fn core_generate_mipmap(&mut self, c: &Command) -> Result<Value> {
        let target = c.u(0)?;
        let slot = self.texture_slot(target)?;
        let id = self.textures[self.texture_unit][slot];
        self.objects.get(id, Kind::Texture)?;
        let mut base = 0;
        let mut maximum = 0;
        unsafe {
            gl::GetTexParameteriv(target, 0x813c, &mut base);
            gl::GetTexParameteriv(target, 0x813d, &mut maximum);
        }
        self.driver_result()?;
        let object = self.objects.get(id, Kind::Texture)?;
        let faces: &[u32] = if slot == 1 {
            &[0x8515, 0x8516, 0x8517, 0x8518, 0x8519, 0x851a]
        } else {
            &[target]
        };
        let mut generated = Vec::new();
        let mut bytes = 0usize;
        for &face in faces {
            let image = *object
                .core_images
                .get(&(face, base))
                .ok_or(gl::INVALID_OPERATION)?;
            if image.width == 0 || image.height == 0 {
                return Err(gl::INVALID_OPERATION);
            }
            let dimension = if slot == 2 {
                image.width.max(image.height).max(image.depth)
            } else {
                image.width.max(image.height)
            };
            let last = (base + (31 - dimension.leading_zeros()) as i32).min(maximum);
            let last = if object.immutable_levels != 0 {
                last.min(object.immutable_levels as i32 - 1)
            } else {
                last
            };
            let (format, kind) = object.texture_images[&(face, base)];
            let (_, storage) = super::core_texture_formats::upload(image.internal, format, kind)?;
            for level in base + 1..=last {
                let shift = level - base;
                let image = Image {
                    internal: image.internal,
                    width: (image.width >> shift).max(1),
                    height: (image.height >> shift).max(1),
                    depth: if slot == 2 {
                        (image.depth >> shift).max(1)
                    } else {
                        image.depth
                    },
                };
                let previous = object
                    .core_images
                    .get(&(face, level))
                    .map(|old| {
                        let (format, kind) = object.texture_images[&(face, level)];
                        super::core_texture_formats::upload(old.internal, format, kind).map(
                            |(_, storage)| {
                                old.width as usize
                                    * old.height as usize
                                    * old.depth as usize
                                    * storage
                            },
                        )
                    })
                    .transpose()?
                    .unwrap_or(0);
                let allocation =
                    image.width as usize * image.height as usize * image.depth as usize * storage;
                bytes = bytes
                    .checked_add(allocation.saturating_sub(previous))
                    .ok_or(gl::OUT_OF_MEMORY)?;
                generated.push(((face, level), image, (format, kind)));
            }
        }
        self.charge(0, bytes)?;
        // GLES3, unlike WebGL1, permits non-power-of-two mip chains and core
        // sRGB formats. ANGLE checks renderability/filterability and cube completeness.
        unsafe {
            gl::GenerateMipmap(target);
        }
        self.driver_result()?;
        let object = self.objects.get_mut(id, Kind::Texture)?;
        object.capacity = object.capacity.saturating_add(bytes);
        for (key, image, format) in generated {
            object.core_images.insert(key, image);
            object.texture_images.insert(key, format);
        }
        Ok(Value::Null)
    }
}
