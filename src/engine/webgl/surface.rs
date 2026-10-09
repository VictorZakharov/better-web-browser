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
    read_buffer: Option<super::core_entries::ReadBuffer>,
    multisample: Option<super::surface_multisample::Multisample>,
    allocated_bytes: usize,
    resolve_dirty: std::cell::Cell<bool>,
    pub(super) presentation_dirty: bool,
}
impl Surface {
    pub fn new(
        width: u32,
        height: u32,
        options: Options,
        core: Option<&super::core_entries::CoreEntries>,
    ) -> Result<Self, String> {
        if width == 0
            || height == 0
            || width > 4096
            || height > 4096
            || u64::from(width) * u64::from(height) > 4 * 1024 * 1024
        {
            return Err("WebGL drawing buffer exceeds the admitted bitmap size".into());
        }
        let allocated_bytes = Self::allocation_bytes(width, height, options)
            .filter(|bytes| *bytes <= super::resource_ceiling())
            .ok_or("WebGL drawing buffer exceeds the context storage budget")?;
        let mut result = Self {
            api: options.api,
            width,
            height,
            framebuffer: 0,
            texture: 0,
            depth_stencil: 0,
            multisample: None,
            allocated_bytes,
            resolve_dirty: std::cell::Cell::new(true),
            presentation_dirty: true,
            read_buffer: if options.api == super::ApiVersion::Two {
                Some(super::core_entries::CoreEntries::read_buffer_entry()?)
            } else {
                None
            },
        };
        let _unpack = super::pixel_buffer_guard::PixelBufferGuard::unbind(
            options.api,
            super::pixel_buffer_guard::Direction::Unpack,
        );
        let _store = super::pixel_store_guard::PixelStoreGuard::tight(
            options.api,
            super::pixel_layout::Direction::Unpack,
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
            if (options.depth || options.stencil) && !super::surface_multisample::requested(options)
            {
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
        if super::surface_multisample::requested(options) {
            let multisample = super::surface_multisample::Multisample::new(
                width,
                height,
                options,
                core.ok_or("Multisampling requires GLES3")?,
                result.framebuffer,
            );
            match multisample {
                Ok(multisample) => {
                    result.framebuffer = multisample.draw;
                    result.multisample = Some(multisample);
                }
                Err(error) => {
                    result.destroy();
                    return Err(error);
                }
            }
        }
        Ok(result)
    }
    pub fn snapshot(&self) -> Result<Vec<u8>, String> {
        self.resolve()?;
        let _pack = super::pixel_buffer_guard::PixelBufferGuard::unbind(
            self.api,
            super::pixel_buffer_guard::Direction::Pack,
        );
        let mut pixels = vec![0; self.width as usize * self.height as usize * 4];
        let _store = super::pixel_store_guard::PixelStoreGuard::tight(
            self.api,
            super::pixel_layout::Direction::Pack,
        );
        let mut alignment = 0;
        let _framebuffer = super::framebuffer_guard::FramebufferGuard::bind(
            self.api,
            super::framebuffer_guard::Direction::Read,
            self.read_surface(),
        );
        let mut route = 0;
        // SAFETY: a bounded, exactly sized RGBA destination. Private state is restored even
        // on readback failure; pixels cannot expose driver memory outside this allocation.
        let error = unsafe {
            gl::GetIntegerv(gl::PACK_ALIGNMENT, &mut alignment);
            if let Some(read_buffer) = self.read_buffer {
                gl::GetIntegerv(0x0c02, &mut route);
                read_buffer(gl::COLOR_ATTACHMENT0);
            }
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
            if let Some(read_buffer) = self.read_buffer {
                read_buffer(route as u32);
            }
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
            if let Some(multisample) = self.multisample.take() {
                self.framebuffer = multisample.resolve;
                multisample.destroy();
            }
            gl::DeleteFramebuffers(1, &self.framebuffer);
            gl::DeleteTextures(1, &self.texture);
            gl::DeleteRenderbuffers(1, &self.depth_stencil);
        }
        self.framebuffer = 0;
        self.texture = 0;
        self.depth_stencil = 0;
    }

    pub(super) fn allocation_bytes(width: u32, height: u32, options: Options) -> Option<usize> {
        let attachments = if options.depth || options.stencil {
            8
        } else {
            4
        };
        let bytes = if super::surface_multisample::requested(options) {
            4 + 4 * attachments
        } else {
            attachments
        };
        (width as usize)
            .checked_mul(height as usize)?
            .checked_mul(bytes)
    }
    pub(super) fn bytes(&self) -> usize {
        self.allocated_bytes
    }
    pub(super) fn read_surface(&self) -> u32 {
        self.multisample
            .as_ref()
            .map_or(self.framebuffer, |surface| surface.resolve)
    }
    pub(super) fn resolve(&self) -> Result<(), String> {
        if self.resolve_dirty.get()
            && let Some(multisample) = &self.multisample
        {
            multisample.resolve(self.width, self.height)?;
            self.resolve_dirty.set(false);
        }
        Ok(())
    }

    pub(super) fn invalidate_resolve(&self) {
        self.resolve_dirty.set(true);
    }

    pub(super) fn resolved_read_guard(
        &self,
        route: u32,
    ) -> Result<Option<super::framebuffer_guard::FramebufferGuard>, String> {
        let Some(multisample) = &self.multisample else {
            return Ok(None);
        };
        self.resolve()?;
        let guard = super::framebuffer_guard::FramebufferGuard::bind(
            self.api,
            super::framebuffer_guard::Direction::Read,
            multisample.resolve,
        );
        unsafe {
            (self.read_buffer.unwrap())(if route == gl::NONE {
                gl::NONE
            } else {
                gl::COLOR_ATTACHMENT0
            });
        }
        Ok(Some(guard))
    }
}
