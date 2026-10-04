//! Private native framebuffer blits avoid ANGLE's 2D-to-3D staging-copy path.
//! No CPU pixel readback, synthesized pixels, or author-visible framebuffer.
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
            // Non-renderable destination formats need a separate conversion
            // path before public admission. Never fall back to the corrupting
            // native CopyTexSubImage3D staging operation for those images.
            return Err(gl::INVALID_OPERATION);
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
