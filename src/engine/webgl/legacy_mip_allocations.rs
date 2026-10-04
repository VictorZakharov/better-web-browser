//! WebGL1 mip regeneration shares upload lifetime high-water accounting.
use super::core_textures::Image;
use super::{Kind, Result, WebGl, gl};
use serde_json::Value;

impl WebGl {
    pub(super) fn generate_legacy_mips(
        &mut self,
        target: u32,
        slot: usize,
        id: u32,
    ) -> Result<Value> {
        let object = self.objects.get(id, Kind::Texture)?;
        let faces: &[u32] = if slot == 1 {
            &[0x8515, 0x8516, 0x8517, 0x8518, 0x8519, 0x851a]
        } else {
            &[target]
        };
        let mut generated = Vec::new();
        let mut allocations = Vec::new();
        for &face in faces {
            let base = *object
                .core_images
                .get(&(face, 0))
                .ok_or(gl::INVALID_OPERATION)?;
            let (format, kind) = object.texture_images[&(face, 0)];
            let pixel =
                super::texture_formats::texture_format(format, kind, &self.extensions.textures)?;
            if base.width == 0
                || base.height == 0
                || !base.width.is_power_of_two()
                || !base.height.is_power_of_two()
            {
                return Err(gl::INVALID_OPERATION);
            }
            let last = base.width.max(base.height).ilog2();
            for level in 1..=last {
                let image = Image {
                    internal: format,
                    width: (base.width >> level).max(1),
                    height: (base.height >> level).max(1),
                    depth: 1,
                };
                allocations.push((
                    (face, level as i32),
                    image.width as usize * image.height as usize * pixel.storage_bytes,
                ));
                generated.push(((face, level as i32), image, (format, kind)));
            }
        }
        let reservation = self.prepare_texture_storage(id, allocations)?;
        // Native validation still decides cube completeness and filterability.
        // Failed generation never commits budget or image metadata.
        unsafe {
            gl::GenerateMipmap(target);
        }
        self.driver_result()?;
        self.commit_texture_storage(reservation)?;
        let object = self.objects.get_mut(id, Kind::Texture)?;
        for (key, image, pair) in generated {
            object.core_images.insert(key, image);
            object.texture_images.insert(key, pair);
        }
        Ok(Value::Null)
    }
}
