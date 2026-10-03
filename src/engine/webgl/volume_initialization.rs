//! Initialize every slice before a layer can mark the entire image initialized.
//! The pinned ANGLE D3D11 backend's lazy framebuffer initialization tracks a
//! whole image, not independently cleared array layers. Never rely on that for
//! untouched layers after framebufferTextureLayer/clear.
use super::{Result, WebGl, gl};

impl WebGl {
    pub(super) fn initialize_volume(
        &mut self,
        target: u32,
        level: i32,
        image: super::core_textures::Image,
    ) -> Result<()> {
        if image.width == 0 || image.height == 0 || image.depth == 0 {
            return Ok(());
        }
        let format = if [
            gl::ALPHA,
            gl::LUMINANCE,
            gl::LUMINANCE_ALPHA,
            gl::RGB,
            gl::RGBA,
        ]
        .contains(&image.internal)
        {
            super::core_texture_formats::StorageFormat {
                base: image.internal,
                types: &[gl::UNSIGNED_BYTE],
                bytes: 4,
            }
        } else {
            super::core_texture_formats::storage(image.internal)?
        };
        let kind = format.types[0];
        let (bytes, _) = super::core_texture_formats::upload(image.internal, format.base, kind)?;
        // Reuse a bounded tile, rather than allocating the entire volume or
        // issuing one driver call per pixel row of a large shadow map.
        let row_bytes = image.width as usize * bytes;
        let rows = (262_144 / row_bytes).max(1).min(image.height as usize);
        let tile = vec![0u8; row_bytes * rows];
        let mut alignment = 0;
        unsafe { gl::GetIntegerv(gl::UNPACK_ALIGNMENT, &mut alignment) };
        let _buffers = super::pixel_buffer_guard::PixelBufferGuard::unbind(
            self.options.api,
            super::pixel_buffer_guard::Direction::Unpack,
        );
        unsafe { gl::PixelStorei(gl::UNPACK_ALIGNMENT, 1) };
        let function = self
            .core
            .as_ref()
            .ok_or(gl::INVALID_OPERATION)?
            .texture_sub_image_3d;
        for layer in 0..image.depth {
            for y in (0..image.height).step_by(rows) {
                unsafe {
                    function(
                        target,
                        level,
                        0,
                        y as i32,
                        layer as i32,
                        image.width as i32,
                        rows.min((image.height - y) as usize) as i32,
                        1,
                        format.base,
                        kind,
                        tile.as_ptr().cast(),
                    );
                }
            }
        }
        unsafe { gl::PixelStorei(gl::UNPACK_ALIGNMENT, alignment) };
        self.driver_result()
    }
}
