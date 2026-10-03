//! WebGL1 has three distinct logical depth/stencil attachment points, unlike
//! GLES3's alias for a combined attachment. Retain each logical assignment so
//! invalid combinations are observable and detachment can recover correctly.
use super::{Command, Kind, Result, WebGl, gl};
use std::collections::HashMap;

pub(super) const DEPTH_STENCIL_ATTACHMENT: u32 = 0x821a;

#[derive(Clone, Copy)]
pub(super) struct Attachment {
    pub id: u32,
    pub native: u32,
    pub kind: Kind,
    pub target: u32,
}

impl WebGl {
    pub(super) fn attach_framebuffer(&mut self, c: &Command) -> Result<()> {
        if c.u(0)? != gl::FRAMEBUFFER {
            return Err(gl::INVALID_ENUM);
        }
        if self.framebuffer == 0 {
            return Err(gl::INVALID_OPERATION);
        }
        let point = c.u(1)?;
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
        let texture = c.op == "framebufferTexture2D";
        let target = c.u(2)?;
        if texture {
            if target != gl::TEXTURE_2D
                && !(gl::TEXTURE_CUBE_MAP_POSITIVE_X..=gl::TEXTURE_CUBE_MAP_NEGATIVE_Z)
                    .contains(&target)
            {
                return Err(gl::INVALID_ENUM);
            }
            if c.n(4)? != 0 {
                return Err(gl::INVALID_VALUE);
            }
        } else if target != gl::RENDERBUFFER {
            return Err(gl::INVALID_ENUM);
        }
        let kind = if texture {
            Kind::Texture
        } else {
            Kind::Renderbuffer
        };
        let id = c.u(3)?;
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
                target,
            })
        };
        let previous = self
            .objects
            .get(self.framebuffer, Kind::Framebuffer)?
            .framebuffer_attachments
            .clone();
        let mut updated = previous.clone();
        if let Some(attachment) = attachment {
            updated.insert(point, attachment);
        } else {
            updated.remove(&point);
        }
        apply(&updated, self.extensions.max_color_attachments);
        if let Err(error) = self.driver_result() {
            apply(&previous, self.extensions.max_color_attachments);
            self.driver_result()?;
            return Err(error);
        }
        if let Some(attachment) = attachment {
            self.objects.retain(attachment.id, attachment.kind)?;
        }
        self.objects
            .get_mut(self.framebuffer, Kind::Framebuffer)?
            .framebuffer_attachments = updated;
        if let Some(old) = previous.get(&point) {
            self.objects.release(old.id);
        }
        Ok(())
    }

    pub(super) fn detach_current_resource(&mut self, id: u32, kind: Kind) -> Result<()> {
        if self.framebuffer == 0 || id == 0 {
            return Ok(());
        }
        let entries = &mut self
            .objects
            .get_mut(self.framebuffer, Kind::Framebuffer)?
            .framebuffer_attachments;
        let removed: Vec<_> = entries
            .iter()
            .filter_map(|(&point, entry)| (entry.id == id && entry.kind == kind).then_some(point))
            .collect();
        if removed.is_empty() {
            return Ok(());
        }
        for point in &removed {
            entries.remove(point);
        }
        apply(entries, self.extensions.max_color_attachments);
        self.driver_result()?;
        for _ in removed {
            self.objects.release(id);
        }
        Ok(())
    }
}

fn apply(entries: &HashMap<u32, Attachment>, color_count: u32) {
    // GLES2 WebGL compatibility also retains distinct logical native slots.
    // Apply each exact assignment without clearing a separate surviving image
    // when removing a combined depth/stencil attachment.
    let colors = (0..color_count.max(1)).map(|index| {
        let point = gl::COLOR_ATTACHMENT0 + index;
        (point, entries.get(&point))
    });
    for (point, entry) in colors.chain([
        (gl::DEPTH_ATTACHMENT, entries.get(&gl::DEPTH_ATTACHMENT)),
        (gl::STENCIL_ATTACHMENT, entries.get(&gl::STENCIL_ATTACHMENT)),
        (
            DEPTH_STENCIL_ATTACHMENT,
            entries.get(&DEPTH_STENCIL_ATTACHMENT),
        ),
    ]) {
        unsafe {
            if let Some(entry) = entry
                && entry.kind == Kind::Texture
            {
                gl::FramebufferTexture2D(gl::FRAMEBUFFER, point, entry.target, entry.native, 0);
            } else {
                gl::FramebufferRenderbuffer(
                    gl::FRAMEBUFFER,
                    point,
                    gl::RENDERBUFFER,
                    entry.map_or(0, |entry| entry.native),
                );
            }
        }
    }
}
