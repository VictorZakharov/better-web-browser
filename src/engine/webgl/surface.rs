//! Private drawing buffer: author framebuffer zero maps here, never to EGL's pbuffer.
use super::{Options, gl};
use std::ptr;

pub(super) struct Surface {
    api: super::ApiVersion,
    pub width: u32,
    pub height: u32,
    pub framebuffer: u32,
    texture: u32,
    depth_stencil: u32,
}
impl Surface {
    pub fn new(width: u32, height: u32, options: Options) -> Result<Self, String> {
        if width == 0
            || height == 0
            || width > 4096
            || height > 4096
            || u64::from(width) * u64::from(height) > 4 * 1024 * 1024
        {
            return Err("WebGL drawing buffer exceeds the admitted bitmap size".into());
        }
        let mut result = Self {
            api: options.api,
            width,
            height,
            framebuffer: 0,
            texture: 0,
            depth_stencil: 0,
        };
        let _unpack = super::pixel_buffer_guard::PixelBufferGuard::unbind(
            options.api,
            super::pixel_buffer_guard::Direction::Unpack,
        );
        // SAFETY: dimensions are bounded, null texture data allocates storage, and ANGLE's
        // robust resource initialization is enabled. Clear also initializes depth/stencil.
        unsafe {
            let format = if options.alpha { gl::RGBA } else { gl::RGB };
            gl::GenTextures(1, &mut result.texture);
            gl::BindTexture(gl::TEXTURE_2D, result.texture);
            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MIN_FILTER, gl::NEAREST as i32);
            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MAG_FILTER, gl::NEAREST as i32);
            gl::TexImage2D(
                gl::TEXTURE_2D,
                0,
                format as i32,
                width as i32,
                height as i32,
                0,
                format,
                gl::UNSIGNED_BYTE,
                ptr::null(),
            );
            gl::GenFramebuffers(1, &mut result.framebuffer);
            gl::BindFramebuffer(gl::FRAMEBUFFER, result.framebuffer);
            gl::FramebufferTexture2D(
                gl::FRAMEBUFFER,
                gl::COLOR_ATTACHMENT0,
                gl::TEXTURE_2D,
                result.texture,
                0,
            );
            if options.depth || options.stencil {
                gl::GenRenderbuffers(1, &mut result.depth_stencil);
                gl::BindRenderbuffer(gl::RENDERBUFFER, result.depth_stencil);
                // Match physical storage to the granted attributes: hidden
                // stencil bits must not make STENCIL_TEST affect a depth-only
                // default surface, and vice versa.
                let storage = if options.depth && options.stencil {
                    0x88f0
                } else if options.depth {
                    0x81a6 // DEPTH_COMPONENT24 via the private OES_depth24 dependency.
                } else {
                    gl::STENCIL_INDEX8
                };
                gl::RenderbufferStorage(gl::RENDERBUFFER, storage, width as i32, height as i32);
                if options.depth && options.stencil {
                    gl::FramebufferRenderbuffer(
                        gl::FRAMEBUFFER,
                        0x821a,
                        gl::RENDERBUFFER,
                        result.depth_stencil,
                    );
                } else if options.depth {
                    gl::FramebufferRenderbuffer(
                        gl::FRAMEBUFFER,
                        gl::DEPTH_ATTACHMENT,
                        gl::RENDERBUFFER,
                        result.depth_stencil,
                    );
                }
                if options.stencil && !options.depth {
                    gl::FramebufferRenderbuffer(
                        gl::FRAMEBUFFER,
                        gl::STENCIL_ATTACHMENT,
                        gl::RENDERBUFFER,
                        result.depth_stencil,
                    );
                }
            }
            let status = gl::CheckFramebufferStatus(gl::FRAMEBUFFER);
            let complete = status == gl::FRAMEBUFFER_COMPLETE;
            let error = gl::GetError();
            if !complete || error != gl::NO_ERROR {
                result.destroy();
                return Err(format!(
                    "WebGL drawing-buffer allocation failed: framebuffer {status:#x}, GL {error:#x}"
                ));
            }
            gl::Viewport(0, 0, width as i32, height as i32);
            gl::ClearColor(0.0, 0.0, 0.0, if options.alpha { 0.0 } else { 1.0 });
            gl::Clear(gl::COLOR_BUFFER_BIT | gl::DEPTH_BUFFER_BIT | gl::STENCIL_BUFFER_BIT);
            gl::BindTexture(gl::TEXTURE_2D, 0);
            gl::BindRenderbuffer(gl::RENDERBUFFER, 0);
        }
        Ok(result)
    }
    pub fn snapshot(&self) -> Result<Vec<u8>, String> {
        let _pack = super::pixel_buffer_guard::PixelBufferGuard::unbind(
            self.api,
            super::pixel_buffer_guard::Direction::Pack,
        );
        let mut pixels = vec![0; self.width as usize * self.height as usize * 4];
        let mut binding = 0;
        let mut alignment = 0;
        // SAFETY: a bounded, exactly sized RGBA destination. Private state is restored even
        // on readback failure; pixels cannot expose driver memory outside this allocation.
        let error = unsafe {
            gl::GetIntegerv(gl::FRAMEBUFFER_BINDING, &mut binding);
            gl::GetIntegerv(gl::PACK_ALIGNMENT, &mut alignment);
            gl::BindFramebuffer(gl::FRAMEBUFFER, self.framebuffer);
            gl::PixelStorei(gl::PACK_ALIGNMENT, 1);
            gl::ReadPixels(
                0,
                0,
                self.width as i32,
                self.height as i32,
                gl::RGBA,
                gl::UNSIGNED_BYTE,
                pixels.as_mut_ptr().cast(),
            );
            let error = gl::GetError();
            gl::PixelStorei(gl::PACK_ALIGNMENT, alignment);
            gl::BindFramebuffer(gl::FRAMEBUFFER, binding as u32);
            error
        };
        if error != gl::NO_ERROR {
            return Err(format!("WebGL readback failed: GL {error:#x}"));
        }
        let stride = self.width as usize * 4;
        for y in 0..self.height as usize / 2 {
            let opposite = self.height as usize - 1 - y;
            let (before, after) = pixels.split_at_mut(opposite * stride);
            before[y * stride..(y + 1) * stride].swap_with_slice(&mut after[..stride]);
        }
        Ok(pixels)
    }
    pub fn destroy(&mut self) {
        // SAFETY: the owner makes its context current before teardown. Zero names are legal.
        unsafe {
            gl::DeleteFramebuffers(1, &self.framebuffer);
            gl::DeleteTextures(1, &self.texture);
            gl::DeleteRenderbuffers(1, &self.depth_stencil);
        }
        self.framebuffer = 0;
        self.texture = 0;
        self.depth_stencil = 0;
    }
}
