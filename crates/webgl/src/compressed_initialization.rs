//! Explicit bounded block initialization covers every immutable array layer.
use super::{Result, WebGl, gl};
impl WebGl {
    pub(super) fn initialize_compressed_image(
        &mut self,
        target: u32,
        level: i32,
        image: super::core_textures::Image,
        array: bool,
    ) -> Result<()> {
        let format = super::compressed_formats::format(image.internal)?;
        let row = image.width.div_ceil(4) as usize * format.block_bytes;
        let block_rows = (262_144 / row)
            .max(1)
            .min(image.height.div_ceil(4) as usize);
        let mut tile = vec![0u8; row * block_rows];
        // BC1 RGBA's all-zero block is opaque. Index 3 in three-color mode
        // encodes transparent zero without a separate decompression implementation.
        if matches!(image.internal, 0x83f1 | 0x8c4d) {
            for block in tile.chunks_exact_mut(8) {
                block[4..].fill(255);
            }
        }
        let _store = super::pixel_store_guard::PixelStoreGuard::tight(
            self.options.api,
            super::pixel_layout::Direction::Unpack,
        );
        let _buffers = super::pixel_buffer_guard::PixelBufferGuard::unbind(
            self.options.api,
            super::pixel_buffer_guard::Direction::Unpack,
        );
        for layer in 0..image.depth {
            for y in (0..image.height).step_by(block_rows * 4) {
                let height = (block_rows as u32 * 4).min(image.height - y);
                let size = format.byte_size(image.width, height)?;
                unsafe {
                    if array {
                        (self
                            .core
                            .as_ref()
                            .ok_or(gl::INVALID_OPERATION)?
                            .compressed_sub_image_3d)(
                            target,
                            level,
                            0,
                            y as i32,
                            layer as i32,
                            image.width as i32,
                            height as i32,
                            1,
                            image.internal,
                            size as i32,
                            tile.as_ptr().cast(),
                        );
                    } else {
                        gl::CompressedTexSubImage2D(
                            target,
                            level,
                            0,
                            y as i32,
                            image.width as i32,
                            height as i32,
                            image.internal,
                            size as i32,
                            tile.as_ptr().cast(),
                        );
                    }
                }
                self.driver_result()?;
            }
        }
        Ok(())
    }
}
