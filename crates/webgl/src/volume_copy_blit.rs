//! Private native framebuffer blits avoid ANGLE's 2D-to-3D staging-copy path.
//! Non-renderable destinations use bounded real-pixel readback/upload instead.
use super::framebuffer_guard::{DRAW, Direction, FramebufferGuard};
use super::{Kind, Result, WebGl, gl};

struct ScratchFramebuffer(u32);
impl Drop for ScratchFramebuffer {
    fn drop(&mut self) {
        unsafe {
            gl::DeleteFramebuffers(1, &self.0);
        }
    }
}

struct Scissor(bool);
impl Scissor {
    fn disabled() -> Self {
        let enabled = unsafe { gl::IsEnabled(gl::SCISSOR_TEST) } != 0;
        unsafe {
            gl::Disable(gl::SCISSOR_TEST);
        }
        Self(enabled)
    }
}
impl Drop for Scissor {
    fn drop(&mut self) {
        if self.0 {
            unsafe {
                gl::Enable(gl::SCISSOR_TEST);
            }
        }
    }
}

impl WebGl {
    pub(super) fn copy_framebuffer_to_volume(
        &mut self,
        id: u32,
        level: i32,
        offsets: [i32; 3],
        source: [i32; 4],
    ) -> Result<()> {
        let internal = self
            .objects
            .get(id, Kind::Texture)?
            .core_images
            .get(&(super::texture_targets::VOLUME, level))
            .ok_or(gl::INVALID_OPERATION)?
            .internal;
        if matches!(
            internal,
            0x8054 | 0x8815 | 0x881b | 0x8d7d | 0x8d8f | 0x8d77 | 0x8d89 | 0x8d71 | 0x8d83
        ) {
            return self.copy_volume_through_native_conversion(id, level, offsets, source);
        }
        let Some((source, offsets)) =
            super::volume_copy_region::clip(source, offsets, self.volume_copy_read_extent()?)
        else {
            return Ok(());
        };
        // The pinned D3D11 volume-source blit can AV or lose its device even
        // when destination storage is distinct. Capture volume reads with
        // native CopyTexImage2D before blitting from that separate GPU image.
        // Same-image feedback was rejected by native validation.
        if self.read_framebuffer != 0 {
            let framebuffer = self.objects.get(self.read_framebuffer, Kind::Framebuffer)?;
            if framebuffer
                .framebuffer_attachments
                .get(&framebuffer.read_buffer)
                .is_some_and(|attachment| {
                    attachment.kind == Kind::Texture
                        && attachment.target == super::texture_targets::VOLUME
                })
            {
                return self.copy_volume_through_gpu_staging(id, level, offsets, source);
            }
        }
        let core = self.core.as_ref().ok_or(gl::INVALID_OPERATION)?;
        let texture = self.objects.name(id, Kind::Texture)?;
        let mut scratch = ScratchFramebuffer(0);
        unsafe {
            gl::GenFramebuffers(1, &mut scratch.0);
        }
        if scratch.0 == 0 {
            return Err(gl::OUT_OF_MEMORY);
        }
        let _draw = FramebufferGuard::bind(self.options.api, Direction::Draw, scratch.0);
        let _scissor = Scissor::disabled();
        unsafe {
            (core.framebuffer_layer)(DRAW, gl::COLOR_ATTACHMENT0, texture, level, offsets[2]);
        }
        self.driver_result()?;
        if unsafe { gl::CheckFramebufferStatus(DRAW) } != gl::FRAMEBUFFER_COMPLETE {
            // Only renderable destinations can use a framebuffer blit. The
            // conversion fallback uploads the copied region without replacing
            // existing volume storage or synthesizing destination pixels.
            return self.copy_volume_through_native_conversion(id, level, offsets, source);
        }
        let [x, y, width, height] = source;
        let [dx, dy, _] = offsets;
        let right = x.checked_add(width).ok_or(gl::INVALID_VALUE)?;
        let bottom = y.checked_add(height).ok_or(gl::INVALID_VALUE)?;
        unsafe {
            (self.core.as_ref().unwrap().blit)(
                x,
                y,
                right,
                bottom,
                dx,
                dy,
                dx + width,
                dy + height,
                gl::COLOR_BUFFER_BIT,
                gl::NEAREST,
            );
        }
        self.driver_result()
    }
}
