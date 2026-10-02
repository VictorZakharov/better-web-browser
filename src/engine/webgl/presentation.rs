//! Compositor retirement clears an unpreserved drawing buffer without changing author state.
use super::{WebGl, gl};

impl WebGl {
    pub(super) fn presented(&mut self) {
        if self.options.preserve {
            return;
        }
        let mut framebuffer = 0;
        let mut color = [0.0; 4];
        let mut depth = 0.0;
        let mut stencil = 0;
        let mut color_mask = [0; 4];
        let mut depth_mask = 0;
        let mut front_mask = 0;
        let mut back_mask = 0;
        // SAFETY: all get calls use their exact scalar/vector types and lengths. Clearing
        // the private drawing buffer is independent of the author's scissor and write masks.
        unsafe {
            gl::GetIntegerv(gl::FRAMEBUFFER_BINDING, &mut framebuffer);
            gl::GetFloatv(gl::COLOR_CLEAR_VALUE, color.as_mut_ptr());
            gl::GetFloatv(gl::DEPTH_CLEAR_VALUE, &mut depth);
            gl::GetIntegerv(gl::STENCIL_CLEAR_VALUE, &mut stencil);
            gl::GetBooleanv(gl::COLOR_WRITEMASK, color_mask.as_mut_ptr());
            gl::GetBooleanv(gl::DEPTH_WRITEMASK, &mut depth_mask);
            gl::GetIntegerv(gl::STENCIL_WRITEMASK, &mut front_mask);
            gl::GetIntegerv(gl::STENCIL_BACK_WRITEMASK, &mut back_mask);
            let scissor = gl::IsEnabled(gl::SCISSOR_TEST);
            gl::BindFramebuffer(gl::FRAMEBUFFER, self.surface.framebuffer);
            gl::Disable(gl::SCISSOR_TEST);
            gl::ColorMask(1, 1, 1, 1);
            gl::DepthMask(1);
            gl::StencilMask(u32::MAX);
            gl::ClearColor(0.0, 0.0, 0.0, if self.options.alpha { 0.0 } else { 1.0 });
            gl::ClearDepthf(1.0);
            gl::ClearStencil(0);
            gl::Clear(gl::COLOR_BUFFER_BIT | gl::DEPTH_BUFFER_BIT | gl::STENCIL_BUFFER_BIT);
            gl::ClearColor(color[0], color[1], color[2], color[3]);
            gl::ClearDepthf(depth);
            gl::ClearStencil(stencil);
            gl::ColorMask(color_mask[0], color_mask[1], color_mask[2], color_mask[3]);
            gl::DepthMask(depth_mask);
            gl::StencilMaskSeparate(gl::FRONT, front_mask as u32);
            gl::StencilMaskSeparate(gl::BACK, back_mask as u32);
            if scissor != 0 {
                gl::Enable(gl::SCISSOR_TEST);
            }
            gl::BindFramebuffer(gl::FRAMEBUFFER, framebuffer as u32);
        }
    }
}
