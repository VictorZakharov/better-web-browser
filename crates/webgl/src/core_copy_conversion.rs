//! The pinned provider retains source alpha in emulated RGB CopyTex images.
//! Re-upload only actual RGB components so sampling's missing alpha is one.
//! Native copies still validate classes, sizes, MSAA and feedback before this
//! bounded transport runs. No guessed pixels or retained CPU mirror are used.
use super::pixel_buffer_guard::{Direction as BufferDirection, PixelBufferGuard};
use super::pixel_layout::Direction as StoreDirection;
use super::pixel_store_guard::PixelStoreGuard;
use super::volume_copy_conversion::Transfer;
use super::{MAX_UPLOAD_BYTES, Result, WebGl, gl};

impl WebGl {
    pub(super) fn validate_core_copy_workspace(
        &self,
        plan: Transfer,
        dimensions: [i32; 2],
        retained: usize,
    ) -> Result<()> {
        let [width, height] = dimensions;
        let size = (width as usize)
            .checked_mul(height as usize)
            .and_then(|pixels| pixels.checked_mul(plan.read_bytes() + 3 * plan.bytes()))
            .ok_or(gl::OUT_OF_MEMORY)?;
        if size > MAX_UPLOAD_BYTES
            || retained
                .checked_add(size)
                .is_none_or(|total| total > self.resource_limit)
        {
            return Err(gl::OUT_OF_MEMORY);
        }
        Ok(())
    }

    pub(super) fn correct_rgb_copy_alpha(
        &mut self,
        target: u32,
        level: i32,
        plan: Transfer,
        source: [i32; 4],
        offsets: [i32; 2],
        sub: bool,
    ) -> Result<()> {
        let (source, offsets) = if sub {
            let Some((source, offsets)) = super::volume_copy_region::clip(
                source,
                [offsets[0], offsets[1], 0],
                self.volume_copy_read_extent()?,
            ) else {
                return Ok(());
            };
            (source, [offsets[0], offsets[1]])
        } else {
            (source, offsets)
        };
        let [x, y, width, height] = source;
        if width == 0 || height == 0 {
            return Ok(());
        }
        let _pack = PixelStoreGuard::tight(self.options.api, StoreDirection::Pack);
        let _unpack = PixelStoreGuard::tight(self.options.api, StoreDirection::Unpack);
        let _pack_buffer = PixelBufferGuard::unbind(self.options.api, BufferDirection::Pack);
        let _unpack_buffer = PixelBufferGuard::unbind(self.options.api, BufferDirection::Unpack);
        self.core_read_pair(plan.read_format(), plan.read_type())?;
        // Capacity was checked before the native copy changed image storage.
        // Zero initialization also makes out-of-framebuffer full copies safe.
        let mut source = vec![0u8; width as usize * height as usize * plan.read_bytes()];
        unsafe {
            gl::ReadPixels(
                x,
                y,
                width,
                height,
                plan.read_format(),
                plan.read_type(),
                source.as_mut_ptr().cast(),
            );
        }
        self.driver_result()?;
        let bytes = plan.select_rgb(&source)?;
        // SAFETY: the same bounded image/subregion was defined by the successful
        // GPU copy. RGB transport drops only alpha, preserving typed components.
        unsafe {
            gl::TexSubImage2D(
                target,
                level,
                offsets[0],
                offsets[1],
                width,
                height,
                plan.upload_format(),
                plan.upload_type(),
                bytes.as_ptr().cast(),
            );
        }
        self.driver_result()
    }
}
