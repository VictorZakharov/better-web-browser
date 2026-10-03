//! Drawing-buffer resize does not recreate author objects or change the viewport.
use super::{Command, Result, Surface, WebGl, gl};
use serde_json::Value;
impl WebGl {
    pub(super) fn resize(&mut self, c: &Command) -> Result<Value> {
        let width = c.u(0)?.max(1);
        let height = c.u(1)?.max(1);
        if width > 4096 || height > 4096 || u64::from(width) * u64::from(height) > 4 * 1024 * 1024 {
            return Err(gl::OUT_OF_MEMORY);
        }
        if width == self.surface.width && height == self.surface.height {
            // Assigning an unchanged canvas dimension still resets its bitmap,
            // but need not allocate overlapping native attachments. Preserve
            // author GL state while performing the mandatory default clear.
            self.clear_default_surface();
            self.driver_result()?;
            return Ok(Value::Null);
        }
        let bpp = if self.options.depth || self.options.stencil {
            8
        } else {
            4
        };
        let previous_bytes = self.surface.width as usize * self.surface.height as usize * bpp;
        let next_bytes = width as usize * height as usize * bpp;
        // Private attachments cannot be referenced by author objects. They are really
        // destroyed on replacement, so unlike author-resource high-water accounting their
        // storage may be reclaimed. Admit the temporary overlap before allocation.
        let peak = self
            .resource_bytes
            .checked_add(next_bytes)
            .ok_or(gl::OUT_OF_MEMORY)?;
        if peak > self.resource_limit {
            return Err(gl::OUT_OF_MEMORY);
        }
        let mut viewport = [0; 4];
        let mut clear = [0.0; 4];
        let mut texture = 0;
        let mut renderbuffer = 0;
        let mut read_framebuffer = 0;
        let previous_surface = self.surface.framebuffer;
        unsafe {
            gl::GetIntegerv(gl::VIEWPORT, viewport.as_mut_ptr());
            gl::GetFloatv(gl::COLOR_CLEAR_VALUE, clear.as_mut_ptr());
            gl::GetIntegerv(gl::TEXTURE_BINDING_2D, &mut texture);
            gl::GetIntegerv(gl::RENDERBUFFER_BINDING, &mut renderbuffer);
            if self.options.api == super::ApiVersion::Two {
                gl::GetIntegerv(
                    super::framebuffer_guard::READ_BINDING,
                    &mut read_framebuffer,
                );
            }
        }
        // Newly allocated surfaces are zero-initialized by ANGLE even with author masks or
        // scissor enabled. Surface::new's explicit clear is redundant in that case.
        let new = Surface::new(width, height, self.options);
        unsafe {
            gl::Viewport(viewport[0], viewport[1], viewport[2], viewport[3]);
            gl::ClearColor(clear[0], clear[1], clear[2], clear[3]);
            gl::BindTexture(gl::TEXTURE_2D, texture as u32);
            gl::BindRenderbuffer(gl::RENDERBUFFER, renderbuffer as u32);
        }
        let allocated = new.is_ok();
        if let Ok(surface) = new {
            self.surface.destroy();
            self.surface = surface;
            self.resource_bytes = self.resource_bytes - previous_bytes + next_bytes;
            self.clear_default_surface();
            if self.options.api == super::ApiVersion::Two && self.default_read_buffer == gl::NONE {
                unsafe {
                    (self.core.as_ref().ok_or(gl::INVALID_OPERATION)?.read_buffer)(gl::NONE);
                }
            }
        }
        let binding = if self.framebuffer == 0 {
            self.surface.framebuffer
        } else {
            self.objects
                .get(self.framebuffer, super::Kind::Framebuffer)?
                .native
        };
        unsafe {
            if self.options.api == super::ApiVersion::Two {
                gl::BindFramebuffer(super::framebuffer_guard::DRAW, binding);
                gl::BindFramebuffer(
                    super::framebuffer_guard::READ,
                    if read_framebuffer as u32 == previous_surface {
                        self.surface.framebuffer
                    } else {
                        read_framebuffer as u32
                    },
                );
            } else {
                gl::BindFramebuffer(gl::FRAMEBUFFER, binding);
            }
        }
        if !allocated {
            return Err(gl::OUT_OF_MEMORY);
        }
        self.driver_result()?;
        Ok(Value::Null)
    }
}
