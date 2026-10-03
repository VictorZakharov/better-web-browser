//! Private GLES3 storage does not broaden WebGL1's color attachment formats.
//! In particular, red/RG images used to emulate legacy alpha/luminance sampling
//! must not acquire GLES3 red/RG renderability through that implementation detail.
use super::{Kind, Result, WebGl, gl};

impl WebGl {
    pub(super) fn framebuffer_status(&self) -> Result<u32> {
        let status = unsafe { gl::CheckFramebufferStatus(gl::FRAMEBUFFER) };
        if self.framebuffer == 0 || status != gl::FRAMEBUFFER_COMPLETE {
            return Ok(status);
        }
        let mut kind = 0;
        unsafe {
            gl::GetFramebufferAttachmentParameteriv(
                gl::FRAMEBUFFER,
                gl::COLOR_ATTACHMENT0,
                gl::FRAMEBUFFER_ATTACHMENT_OBJECT_TYPE,
                &mut kind,
            );
        }
        if kind as u32 != gl::TEXTURE {
            return Ok(status);
        }
        let mut native = 0;
        let mut face = 0;
        unsafe {
            gl::GetFramebufferAttachmentParameteriv(
                gl::FRAMEBUFFER,
                gl::COLOR_ATTACHMENT0,
                gl::FRAMEBUFFER_ATTACHMENT_OBJECT_NAME,
                &mut native,
            );
            gl::GetFramebufferAttachmentParameteriv(
                gl::FRAMEBUFFER,
                gl::COLOR_ATTACHMENT0,
                gl::FRAMEBUFFER_ATTACHMENT_TEXTURE_CUBE_MAP_FACE,
                &mut face,
            );
        }
        let Some(id) = self.objects.public_name(native as u32, Kind::Texture) else {
            return Ok(status);
        };
        let target = if face == 0 {
            gl::TEXTURE_2D
        } else {
            face as u32
        };
        let image = self
            .objects
            .get(id, Kind::Texture)?
            .texture_images
            .get(&(target, 0));
        if image.is_some_and(|(format, _)| ![gl::RGBA, gl::RGB].contains(format)) {
            return Ok(gl::FRAMEBUFFER_INCOMPLETE_ATTACHMENT);
        }
        Ok(status)
    }

    pub(super) fn validate_framebuffer(&self) -> Result<()> {
        if self.framebuffer_status()? == gl::FRAMEBUFFER_COMPLETE {
            Ok(())
        } else {
            Err(gl::INVALID_FRAMEBUFFER_OPERATION)
        }
    }
}
