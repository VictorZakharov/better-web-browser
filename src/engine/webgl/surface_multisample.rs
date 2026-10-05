//! Real four-sample drawing-buffer storage, with a private color-only resolve.
//! GLES3 blits perform the resolve; no CPU averaging or advertised-only MSAA.
use super::core_entries::CoreEntries;
use super::framebuffer_guard::{Direction, FramebufferGuard};
use super::{ApiVersion, Options, gl};

impl super::WebGl {
    pub(super) fn invalidate_default_resolve(&mut self, operation: &str) {
        // Only writes to the private default draw buffer invalidate resolved
        // color. Readback, queries and state changes must not repeatedly submit
        // identical full-surface GPU blits. Conservatively invalidate even when
        // validation, write masks or rasterizer discard later prevent a write.
        if self.framebuffer == 0
            && matches!(
                operation,
                "clear"
                    | "clearBufferfv"
                    | "clearBufferiv"
                    | "clearBufferuiv"
                    | "clearBufferfi"
                    | "drawArrays"
                    | "drawElements"
                    | "drawArraysInstanced"
                    | "drawElementsInstanced"
                    | "drawArraysInstancedANGLE"
                    | "drawElementsInstancedANGLE"
                    | "drawRangeElements"
                    | "blitFramebuffer"
                    | "invalidateFramebuffer"
                    | "invalidateSubFramebuffer"
            )
        {
            self.surface.presentation_dirty = true;
            self.surface.invalidate_resolve();
            self.readback_cache.invalidate();
        }
    }

    pub(super) fn resolved_default_read(&self) -> super::Result<Option<FramebufferGuard>> {
        if self.options.api == ApiVersion::Two && self.read_framebuffer == 0 {
            self.surface
                .resolved_read_guard(self.default_read_buffer)
                .map_err(|_| gl::INVALID_OPERATION)
        } else {
            Ok(None)
        }
    }
}

pub(super) fn requested(options: Options) -> bool {
    options.api == ApiVersion::Two && options.antialias
}

pub(super) struct Multisample {
    pub(super) draw: u32,
    pub(super) resolve: u32,
    color: u32,
    depth: u32,
    blit: super::core_entries::Blit,
    read_buffer: super::core_entries::ReadBuffer,
}

impl Multisample {
    pub(super) fn new(
        width: u32,
        height: u32,
        options: Options,
        core: &CoreEntries,
        resolve: u32,
    ) -> Result<Self, String> {
        let color = if options.alpha { 0x8058 } else { 0x8051 };
        let depth = if options.depth && options.stencil {
            0x88f0
        } else if options.depth {
            0x81a6
        } else {
            gl::STENCIL_INDEX8
        };
        supported(core, color)?;
        if options.depth || options.stencil {
            supported(core, depth)?;
        }
        let mut result = Self {
            draw: 0,
            resolve,
            color: 0,
            depth: 0,
            blit: core.blit,
            read_buffer: core.read_buffer,
        };
        unsafe {
            gl::GenFramebuffers(1, &mut result.draw);
            gl::BindFramebuffer(gl::FRAMEBUFFER, result.draw);
            gl::GenRenderbuffers(1, &mut result.color);
            gl::BindRenderbuffer(gl::RENDERBUFFER, result.color);
            (core.renderbuffer_multisample)(
                gl::RENDERBUFFER,
                4,
                color,
                width as i32,
                height as i32,
            );
            gl::FramebufferRenderbuffer(
                gl::FRAMEBUFFER,
                gl::COLOR_ATTACHMENT0,
                gl::RENDERBUFFER,
                result.color,
            );
            if options.depth || options.stencil {
                gl::GenRenderbuffers(1, &mut result.depth);
                gl::BindRenderbuffer(gl::RENDERBUFFER, result.depth);
                (core.renderbuffer_multisample)(
                    gl::RENDERBUFFER,
                    4,
                    depth,
                    width as i32,
                    height as i32,
                );
                let point = if options.depth && options.stencil {
                    0x821a
                } else if options.depth {
                    gl::DEPTH_ATTACHMENT
                } else {
                    gl::STENCIL_ATTACHMENT
                };
                gl::FramebufferRenderbuffer(gl::FRAMEBUFFER, point, gl::RENDERBUFFER, result.depth);
            }
            let status = gl::CheckFramebufferStatus(gl::FRAMEBUFFER);
            let error = gl::GetError();
            if status != gl::FRAMEBUFFER_COMPLETE || error != gl::NO_ERROR {
                result.destroy();
                return Err(format!(
                    "WebGL multisample allocation failed: framebuffer {status:#x}, GL {error:#x}"
                ));
            }
            gl::Clear(gl::COLOR_BUFFER_BIT | gl::DEPTH_BUFFER_BIT | gl::STENCIL_BUFFER_BIT);
            gl::BindRenderbuffer(gl::RENDERBUFFER, 0);
        }
        Ok(result)
    }

    pub(super) fn resolve(&self, width: u32, height: u32) -> Result<(), String> {
        let _read = FramebufferGuard::bind(ApiVersion::Two, Direction::Read, self.draw);
        let _draw = FramebufferGuard::bind(ApiVersion::Two, Direction::Draw, self.resolve);
        let mut route = 0;
        let scissor = unsafe { gl::IsEnabled(gl::SCISSOR_TEST) };
        unsafe {
            gl::GetIntegerv(0x0c02, &mut route);
            (self.read_buffer)(gl::COLOR_ATTACHMENT0);
            gl::Disable(gl::SCISSOR_TEST);
            (self.blit)(
                0,
                0,
                width as i32,
                height as i32,
                0,
                0,
                width as i32,
                height as i32,
                gl::COLOR_BUFFER_BIT,
                gl::NEAREST,
            );
            let error = gl::GetError();
            (self.read_buffer)(route as u32);
            if scissor != 0 {
                gl::Enable(gl::SCISSOR_TEST);
            }
            if error != gl::NO_ERROR {
                return Err(format!("WebGL multisample resolve failed: GL {error:#x}"));
            }
        }
        Ok(())
    }

    pub(super) fn destroy(&self) {
        unsafe {
            gl::DeleteFramebuffers(1, &self.draw);
            gl::DeleteRenderbuffers(1, &self.color);
            gl::DeleteRenderbuffers(1, &self.depth);
        }
    }
}

fn supported(core: &CoreEntries, format: u32) -> Result<(), String> {
    let mut count = 0;
    unsafe {
        (core.internalformat_query)(gl::RENDERBUFFER, format, 0x9380, 1, &mut count);
    }
    if unsafe { gl::GetError() } != gl::NO_ERROR || !(1..=32).contains(&count) {
        return Err("WebGL multisample format has no bounded sample-count table".into());
    }
    let mut samples = [0; 32];
    unsafe {
        (core.internalformat_query)(
            gl::RENDERBUFFER,
            format,
            0x80a9,
            count,
            samples.as_mut_ptr(),
        );
    }
    if unsafe { gl::GetError() } != gl::NO_ERROR || !samples[..count as usize].contains(&4) {
        return Err(format!(
            "WebGL multisample format {format:#x} does not support four samples"
        ));
    }
    Ok(())
}
