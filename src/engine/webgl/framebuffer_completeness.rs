//! Native capability does not broaden WebGL1's color attachment format contract.
//! Legacy alpha/luminance images are sampleable but not color-renderable.
use super::{Kind, Result, WebGl, gl};

impl WebGl {
    pub(super) fn framebuffer_status(&self) -> Result<u32> {
        if self.framebuffer != 0 {
            let entries = &self
                .objects
                .get(self.framebuffer, Kind::Framebuffer)?
                .framebuffer_attachments;
            let depth_points = [
                gl::DEPTH_ATTACHMENT,
                gl::STENCIL_ATTACHMENT,
                super::framebuffer_attachments::DEPTH_STENCIL_ATTACHMENT,
            ];
            if depth_points
                .iter()
                .filter(|point| entries.contains_key(point))
                .count()
                > 1
            {
                return Ok(gl::FRAMEBUFFER_UNSUPPORTED);
            }
            for point in depth_points {
                if let Some(entry) = entries.get(&point) {
                    let object = self.objects.get(entry.id, entry.kind)?;
                    let format = if entry.kind == Kind::Texture {
                        object
                            .texture_images
                            .get(&(entry.target, 0))
                            .map_or(0, |image| image.0)
                    } else {
                        object.renderbuffer_format
                    };
                    if format == 0 {
                        continue;
                    } // Native status handles undefined storage.
                    let expected = match (point, entry.kind) {
                        (gl::DEPTH_ATTACHMENT, Kind::Texture) => {
                            super::depth_textures::DEPTH_COMPONENT
                        }
                        (gl::DEPTH_ATTACHMENT, _) => gl::DEPTH_COMPONENT16,
                        (gl::STENCIL_ATTACHMENT, Kind::Texture) => {
                            return Ok(gl::FRAMEBUFFER_UNSUPPORTED);
                        }
                        (gl::STENCIL_ATTACHMENT, _) => gl::STENCIL_INDEX8,
                        _ => super::depth_textures::DEPTH_STENCIL,
                    };
                    if format != expected {
                        return Ok(gl::FRAMEBUFFER_UNSUPPORTED);
                    }
                }
            }
        }
        let status = unsafe { gl::CheckFramebufferStatus(gl::FRAMEBUFFER) };
        if self.framebuffer == 0 || status != gl::FRAMEBUFFER_COMPLETE {
            return Ok(status);
        }
        let entries = &self
            .objects
            .get(self.framebuffer, Kind::Framebuffer)?
            .framebuffer_attachments;
        let mut images = std::collections::HashSet::new();
        for (&point, entry) in entries {
            if !self.color_attachment_allowed(point) {
                continue;
            }
            if !images.insert((entry.id, entry.target)) {
                // WebGL rejects aliasing one image through multiple color slots,
                // even if the GLES provider considers that physical FBO complete.
                return Ok(gl::FRAMEBUFFER_UNSUPPORTED);
            }
            if entry.kind != Kind::Texture {
                continue;
            }
            let image = self
                .objects
                .get(entry.id, Kind::Texture)?
                .texture_images
                .get(&(entry.target, 0));
            if image.is_some_and(|(format, _)| {
                ![gl::RGBA, gl::RGB, super::texture_color_space::SRGB_ALPHA].contains(format)
            }) {
                return Ok(gl::FRAMEBUFFER_INCOMPLETE_ATTACHMENT);
            }
        }
        Ok(status)
    }

    pub(super) fn validate_framebuffer(&self) -> Result<()> {
        // Only browser code can allocate/replace the private default surface;
        // successful allocation establishes its completeness until retirement.
        if self.framebuffer == 0 {
            return Ok(());
        }
        if self.framebuffer_status()? == gl::FRAMEBUFFER_COMPLETE {
            Ok(())
        } else {
            Err(gl::INVALID_FRAMEBUFFER_OPERATION)
        }
    }
}
