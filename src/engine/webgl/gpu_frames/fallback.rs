use super::super::{
    framebuffer_guard::{Direction, FramebufferGuard},
    pixel_buffer_guard::{Direction as BufferDirection, PixelBufferGuard},
    pixel_layout::Direction as StoreDirection,
    pixel_store_guard::PixelStoreGuard,
};
use super::*;

impl WebGl {
    /// Recover the last exported frame after compositing declines it, even when
    /// an unpreserved author drawing buffer has already been implicitly cleared.
    pub(in crate::engine::webgl) fn last_gpu_bitmap(
        &mut self,
    ) -> Result<Option<(u32, u32, Vec<u8>)>, String> {
        let Some(pool) = self.gpu_frames.as_ref() else {
            return Ok(None);
        };
        let Some(index) = pool.latest else {
            return Ok(None);
        };
        let slot = &pool.slots[index];
        if !try_acquire(&slot.surface.mutex, 0)? && !try_acquire(&slot.surface.mutex, slot.key)? {
            return Ok(None);
        }
        let read = (|| {
            self.native.make_surface_current(&slot.surface)?;
            let _framebuffer =
                FramebufferGuard::bind(super::super::ApiVersion::Two, Direction::Read, 0);
            let _pack =
                PixelBufferGuard::unbind(super::super::ApiVersion::Two, BufferDirection::Pack);
            let _store =
                PixelStoreGuard::tight(super::super::ApiVersion::Two, StoreDirection::Pack);
            let mut route = 0;
            let core = self.core.as_ref().ok_or("GPU fallback requires GLES3")?;
            let mut pixels = vec![0; pool.width as usize * pool.height as usize * 4];
            unsafe {
                gl::GetIntegerv(0x0c02, &mut route);
                (core.read_buffer)(gl::BACK);
                gl::ReadPixels(
                    0,
                    0,
                    pool.width as i32,
                    pool.height as i32,
                    gl::RGBA,
                    gl::UNSIGNED_BYTE,
                    pixels.as_mut_ptr().cast(),
                );
                let error = gl::GetError();
                (core.read_buffer)(route as u32);
                if error != gl::NO_ERROR {
                    return Err(format!("last GPU frame fallback failed: GL {error:#x}"));
                }
            }
            // The export blit already made this pbuffer top-down. A second row
            // reversal would invert the retained displayed frame.
            if self.options.alpha {
                crate::engine::image_decode::unpremultiply_rgba(&mut pixels);
            } else {
                for pixel in pixels.chunks_exact_mut(4) {
                    pixel[3] = 255;
                }
            }
            Ok(Some((pool.width, pool.height, pixels)))
        })();
        let restored = self.native.make_current();
        let release =
            unsafe { slot.surface.mutex.ReleaseSync(0) }.map_err(|error| error.to_string());
        restored?;
        release?;
        let bitmap = read?;
        self.gpu_frames = None;
        self.native.make_current()?;
        Ok(bitmap)
    }
}
