//! Compositor retirement clears an unpreserved drawing buffer without changing author state.
use super::{WebGl, gl};

impl WebGl {
    pub(super) fn presented(&mut self) {
        // Retirement changes the readable unpreserved buffer, not the last
        // displayed bitmap. FBO-only work must not publish that implicit clear.
        self.surface.presentation_dirty = false;
        if self.options.preserve {
            return;
        }
        self.clear_default_surface();
    }
    pub(super) fn clear_default_surface(&mut self) {
        self.readback_cache.invalidate();
        self.surface.invalidate_resolve();
        let _framebuffer = super::framebuffer_guard::FramebufferGuard::bind(
            self.options.api,
            super::framebuffer_guard::Direction::Draw,
            self.surface.framebuffer,
        );
        let mut color = [0.0; 4];
        let mut depth = 0.0;
        let mut stencil = 0;
        let mut color_mask = [0; 4];
        let mut depth_mask = 0;
        // SAFETY: all get calls use their exact scalar/vector types and lengths. Clearing
        // the private drawing buffer is independent of the author's scissor and write masks.
        unsafe {
            gl::GetFloatv(gl::COLOR_CLEAR_VALUE, color.as_mut_ptr());
            gl::GetFloatv(gl::DEPTH_CLEAR_VALUE, &mut depth);
            gl::GetIntegerv(gl::STENCIL_CLEAR_VALUE, &mut stencil);
            gl::GetBooleanv(gl::COLOR_WRITEMASK, color_mask.as_mut_ptr());
            gl::GetBooleanv(gl::DEPTH_WRITEMASK, &mut depth_mask);
            let scissor = gl::IsEnabled(gl::SCISSOR_TEST);
            // GLES3 ignores clears while rasterizer discard is enabled.
            // WebGL's implicit drawing-buffer retirement must clear regardless
            // of author state (WebGL2: RASTERIZER_DISCARD restriction).
            let discard = self.options.api == super::ApiVersion::Two && gl::IsEnabled(0x8c89) != 0;
            if discard {
                gl::Disable(0x8c89);
            }
            if self.default_draw_buffer == gl::NONE
                && let Some(entry) = self.extensions.draw_buffers_entry
            {
                entry(1, &gl::COLOR_ATTACHMENT0);
            }
            gl::Disable(gl::SCISSOR_TEST);
            gl::ColorMask(1, 1, 1, 1);
            gl::DepthMask(1);
            gl::StencilMask(u32::MAX);
            gl::ClearColor(0.0, 0.0, 0.0, if self.options.alpha { 0.0 } else { 1.0 });
            gl::ClearDepthf(1.0);
            gl::ClearStencil(0);
            gl::Clear(gl::COLOR_BUFFER_BIT | gl::DEPTH_BUFFER_BIT | gl::STENCIL_BUFFER_BIT);
            if self.default_draw_buffer == gl::NONE
                && let Some(entry) = self.extensions.draw_buffers_entry
            {
                entry(1, &gl::NONE);
            }
            gl::ClearColor(color[0], color[1], color[2], color[3]);
            gl::ClearDepthf(depth);
            gl::ClearStencil(stencil);
            gl::ColorMask(color_mask[0], color_mask[1], color_mask[2], color_mask[3]);
            gl::DepthMask(depth_mask);
            gl::StencilMaskSeparate(gl::FRONT, self.stencil_masks.write[0]);
            gl::StencilMaskSeparate(gl::BACK, self.stencil_masks.write[1]);
            if scissor != 0 {
                gl::Enable(gl::SCISSOR_TEST);
            }
            if discard {
                gl::Enable(0x8c89);
            }
        }
    }
}
