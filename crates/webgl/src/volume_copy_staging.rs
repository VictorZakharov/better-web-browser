//! A private GPU image makes valid copies between volume subresources safe.
//! The pinned ANGLE D3D11 volume-source blit crashes or loses the device.
//! GLES3 CopyTexSubImage validation remains authoritative before this path.
use super::core_texture_formats as formats;
use super::framebuffer_guard::{DRAW, Direction, FramebufferGuard};
use super::{Kind, MAX_UPLOAD_BYTES, Result, WebGl, gl};

struct Staging {
    texture: u32,
    framebuffer: u32,
    previous_texture: u32,
    scissor: bool,
}
impl Drop for Staging {
    fn drop(&mut self) {
        unsafe {
            gl::BindTexture(gl::TEXTURE_2D, self.previous_texture);
            gl::DeleteFramebuffers(1, &self.framebuffer);
            gl::DeleteTextures(1, &self.texture);
            if self.scissor {
                gl::Enable(gl::SCISSOR_TEST);
            }
        }
    }
}

impl WebGl {
    pub(super) fn copy_volume_through_gpu_staging(
        &mut self,
        id: u32,
        level: i32,
        offsets: [i32; 3],
        source: [i32; 4],
    ) -> Result<()> {
        let image = self
            .objects
            .get(id, Kind::Texture)?
            .core_images
            .get(&(super::texture_targets::VOLUME, level))
            .ok_or(gl::INVALID_OPERATION)?;
        let internal = image.internal;
        let bytes = if internal == gl::RGBA {
            4
        } else {
            let format = formats::storage(internal)?;
            format.bytes
        };
        let [x, y, width, height] = source;
        let capacity = (width as usize)
            .checked_mul(height as usize)
            .and_then(|pixels| pixels.checked_mul(bytes * 2))
            .ok_or(gl::OUT_OF_MEMORY)?;
        if capacity > MAX_UPLOAD_BYTES
            || self
                .resource_bytes
                .checked_add(capacity)
                .is_none_or(|total| total > self.resource_limit)
        {
            return Err(gl::OUT_OF_MEMORY);
        }
        let destination = self.objects.name(id, Kind::Texture)?;
        let mut previous = 0;
        unsafe {
            gl::GetIntegerv(gl::TEXTURE_BINDING_2D, &mut previous);
        }
        let mut staging = Staging {
            texture: 0,
            framebuffer: 0,
            previous_texture: previous as u32,
            scissor: unsafe { gl::IsEnabled(gl::SCISSOR_TEST) } != 0,
        };
        unsafe {
            gl::GenTextures(1, &mut staging.texture);
            gl::GenFramebuffers(1, &mut staging.framebuffer);
            gl::Disable(gl::SCISSOR_TEST);
            gl::BindTexture(gl::TEXTURE_2D, staging.texture);
            gl::CopyTexImage2D(gl::TEXTURE_2D, 0, internal, x, y, width, height, 0);
        }
        self.driver_result()?;
        if staging.texture == 0 || staging.framebuffer == 0 {
            return Err(gl::OUT_OF_MEMORY);
        }
        let core = self.core.as_ref().ok_or(gl::INVALID_OPERATION)?;
        let (blit, framebuffer_layer) = (core.blit, core.framebuffer_layer);
        let _draw = FramebufferGuard::bind(self.options.api, Direction::Draw, staging.framebuffer);
        unsafe {
            gl::FramebufferTexture2D(
                DRAW,
                gl::COLOR_ATTACHMENT0,
                gl::TEXTURE_2D,
                staging.texture,
                0,
            );
        }
        self.driver_result()?;
        if unsafe { gl::CheckFramebufferStatus(DRAW) } != gl::FRAMEBUFFER_COMPLETE {
            return Err(gl::INVALID_OPERATION);
        }
        // Native CopyTexImage2D captured the clipped source, initializing the
        // entire private image without ANGLE's volume-source blit path.
        let _read = FramebufferGuard::bind(self.options.api, Direction::Read, staging.framebuffer);
        // READ keeps the staging image while a second private DRAW framebuffer
        // attaches the destination. Neither blit aliases a native resource.
        let mut output = 0;
        unsafe {
            gl::GenFramebuffers(1, &mut output);
        }
        if output == 0 {
            return Err(gl::OUT_OF_MEMORY);
        }
        struct Output(u32);
        impl Drop for Output {
            fn drop(&mut self) {
                unsafe {
                    gl::DeleteFramebuffers(1, &self.0);
                }
            }
        }
        let output = Output(output);
        let _output = FramebufferGuard::bind(self.options.api, Direction::Draw, output.0);
        let [dx, dy, dz] = offsets;
        unsafe {
            framebuffer_layer(DRAW, gl::COLOR_ATTACHMENT0, destination, level, dz);
        }
        self.driver_result()?;
        if unsafe { gl::CheckFramebufferStatus(DRAW) } != gl::FRAMEBUFFER_COMPLETE {
            return Err(gl::INVALID_OPERATION);
        }
        unsafe {
            blit(
                0,
                0,
                width,
                height,
                dx,
                dy,
                dx + width,
                dy + height,
                gl::COLOR_BUFFER_BIT,
                gl::NEAREST,
            );
        }
        self.driver_result()?;
        Ok(())
    }
}
