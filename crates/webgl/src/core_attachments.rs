//! GLES3 layered attachments and the depth/stencil alias retain exact image identity.
use super::framebuffer_attachments::{Attachment, DEPTH_STENCIL_ATTACHMENT};
use super::texture_targets::{ARRAY, VOLUME};
use super::{ApiVersion, Command, Kind, Result, WebGl, gl};

impl WebGl {
    pub(super) fn core_detach_resource(&mut self, id: u32, kind: Kind) -> Result<()> {
        if id == 0 {
            return Ok(());
        }
        for (target, framebuffer) in [
            (super::framebuffer_guard::DRAW, self.framebuffer),
            (super::framebuffer_guard::READ, self.read_framebuffer),
        ] {
            if framebuffer == 0 {
                continue;
            }
            let removed: Vec<_> = self
                .objects
                .get(framebuffer, Kind::Framebuffer)?
                .framebuffer_attachments
                .iter()
                .filter_map(|(&point, entry)| {
                    (entry.id == id && entry.kind == kind).then_some(point)
                })
                .collect();
            for &point in &removed {
                self.core_apply_attachment(target, point, None)?;
            }
            self.driver_result()?;
            for point in removed {
                self.objects
                    .get_mut(framebuffer, Kind::Framebuffer)?
                    .framebuffer_attachments
                    .remove(&point);
                self.objects.release(id);
            }
        }
        Ok(())
    }
    pub(super) fn core_attach_framebuffer(&mut self, c: &Command) -> Result<()> {
        if self.options.api != ApiVersion::Two {
            return Err(gl::INVALID_OPERATION);
        }
        let target = c.u(0)?;
        let framebuffer = self.core_framebuffer_id(target)?;
        if framebuffer == 0 {
            return Err(gl::INVALID_OPERATION);
        }
        let point = c.u(1)?;
        let layered = c.op == "framebufferTextureLayer";
        if (gl::COLOR_ATTACHMENT0..=gl::COLOR_ATTACHMENT0 + 31).contains(&point)
            && !self.color_attachment_allowed(point)
        {
            // GLES3's layered attachment command distinguishes a recognized
            // but unavailable color slot. Texture2D/Renderbuffer instead use
            // INVALID_ENUM for points outside MAX_COLOR_ATTACHMENTS.
            // Khronos ES3 reference pages: glFramebufferTexture2D/Layer.
            return Err(if layered {
                gl::INVALID_OPERATION
            } else {
                gl::INVALID_ENUM
            });
        }
        if !self.color_attachment_allowed(point)
            && ![
                gl::DEPTH_ATTACHMENT,
                gl::STENCIL_ATTACHMENT,
                DEPTH_STENCIL_ATTACHMENT,
            ]
            .contains(&point)
        {
            return Err(gl::INVALID_ENUM);
        }
        let texture = layered || c.op == "framebufferTexture2D";
        let kind = if texture {
            Kind::Texture
        } else {
            Kind::Renderbuffer
        };
        let id = c.u(if layered { 2 } else { 3 })?;
        // GLES3.0 §4.4.2: null texture detachment ignores level, face and
        // layer, including negative or otherwise impossible image indices.
        let level = if texture && id != 0 {
            c.n(if layered { 3 } else { 4 })?
        } else {
            0
        };
        if !(0..=12).contains(&level) {
            return Err(gl::INVALID_VALUE);
        }
        let layer = if layered && id != 0 {
            Some(c.n(4)?)
        } else {
            None
        };
        if layer.is_some_and(|layer| layer < 0) {
            return Err(gl::INVALID_VALUE);
        }
        let image_target = if layered {
            if id == 0 {
                VOLUME
            } else {
                self.objects.get(id, kind)?.buffer_target
            }
        } else {
            c.u(2)?
        };
        if texture && id != 0 {
            if layered {
                if ![VOLUME, ARRAY].contains(&image_target) {
                    return Err(gl::INVALID_OPERATION);
                }
                if layer.unwrap() >= if image_target == ARRAY { 256 } else { 4096 } {
                    return Err(gl::INVALID_VALUE);
                }
            } else if image_target != gl::TEXTURE_2D && !(0x8515..=0x851a).contains(&image_target) {
                return Err(gl::INVALID_ENUM);
            }
        } else if !texture && image_target != gl::RENDERBUFFER {
            return Err(gl::INVALID_ENUM);
        }
        let attachment = if id == 0 {
            None
        } else {
            let object = self.objects.get(id, kind)?;
            if object.pending_delete {
                return Err(gl::INVALID_OPERATION);
            }
            Some(Attachment {
                id,
                native: object.native,
                kind,
                target: image_target,
                level,
                layer,
            })
        };
        let points: &[u32] = if point == DEPTH_STENCIL_ATTACHMENT {
            &[gl::DEPTH_ATTACHMENT, gl::STENCIL_ATTACHMENT]
        } else {
            &[point]
        };
        let previous = self
            .objects
            .get(framebuffer, Kind::Framebuffer)?
            .framebuffer_attachments
            .clone();
        for &point in points {
            self.core_apply_attachment(target, point, attachment)?;
        }
        if let Err(error) = self.driver_result() {
            for &point in points {
                self.core_apply_attachment(target, point, previous.get(&point).copied())?;
            }
            self.driver_result()?;
            return Err(error);
        }
        for &point in points {
            if let Some(attachment) = attachment {
                self.objects.retain(attachment.id, attachment.kind)?;
            }
            let entries = &mut self
                .objects
                .get_mut(framebuffer, Kind::Framebuffer)?
                .framebuffer_attachments;
            if let Some(attachment) = attachment {
                entries.insert(point, attachment);
            } else {
                entries.remove(&point);
            }
            if let Some(old) = previous.get(&point) {
                self.objects.release(old.id);
            }
        }
        Ok(())
    }

    pub(super) fn core_apply_attachment(
        &self,
        target: u32,
        point: u32,
        entry: Option<Attachment>,
    ) -> Result<()> {
        // WebGL2 inherits GLES3.0 attachment semantics. Our GLES3.1 provider
        // additionally rejects immutable levels outside allocated storage;
        // GLES3.0 permits the attachment and makes the framebuffer incomplete.
        // Keep the exact browser image identity, but detach this nonexistent
        // native image so the stricter provider cannot retain an old image.
        let entry = if let Some(entry) = entry {
            if entry.kind == Kind::Texture {
                let object = self.objects.get(entry.id, Kind::Texture)?;
                if object.immutable_levels != 0 && entry.level as u32 >= object.immutable_levels {
                    None
                } else {
                    Some(entry)
                }
            } else {
                Some(entry)
            }
        } else {
            None
        };
        unsafe {
            match entry {
                Some(entry) if entry.kind == Kind::Texture && entry.layer.is_some() => {
                    (self
                        .core
                        .as_ref()
                        .ok_or(gl::INVALID_OPERATION)?
                        .framebuffer_layer)(
                        target,
                        point,
                        entry.native,
                        entry.level,
                        entry.layer.unwrap(),
                    );
                }
                Some(entry) if entry.kind == Kind::Texture => {
                    gl::FramebufferTexture2D(target, point, entry.target, entry.native, entry.level)
                }
                _ => gl::FramebufferRenderbuffer(
                    target,
                    point,
                    gl::RENDERBUFFER,
                    entry.map_or(0, |entry| entry.native),
                ),
            }
        }
        Ok(())
    }
}
