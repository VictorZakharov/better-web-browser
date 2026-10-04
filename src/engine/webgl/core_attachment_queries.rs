//! Sized/layered framebuffer reflection returns browser identities, not native names.
use super::framebuffer_attachments::DEPTH_STENCIL_ATTACHMENT;
use super::{Command, Kind, Result, WebGl, gl, json};
use serde_json::Value;

impl WebGl {
    pub(super) fn core_attachment_parameter(&mut self, c: &Command) -> Result<Value> {
        let target = c.u(0)?;
        let framebuffer = self.core_framebuffer_id(target)?;
        if framebuffer == 0 {
            return self.default_attachment_parameter(c);
        }
        let point = c.u(1)?;
        if [gl::BACK, 0x1801, 0x1802].contains(&point) {
            return Err(gl::INVALID_OPERATION);
        }
        if (gl::COLOR_ATTACHMENT0..=gl::COLOR_ATTACHMENT0 + 31).contains(&point)
            && !self.color_attachment_allowed(point)
        {
            return Err(gl::INVALID_OPERATION);
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
        let pname = c.u(2)?;
        let valid_pname = [
            gl::FRAMEBUFFER_ATTACHMENT_OBJECT_TYPE,
            gl::FRAMEBUFFER_ATTACHMENT_OBJECT_NAME,
            gl::FRAMEBUFFER_ATTACHMENT_TEXTURE_LEVEL,
            gl::FRAMEBUFFER_ATTACHMENT_TEXTURE_CUBE_MAP_FACE,
            0x8cd4,
            0x8210,
            0x8211,
            0x8212,
            0x8213,
            0x8214,
            0x8215,
            0x8216,
            0x8217,
        ]
        .contains(&pname);
        let entries = &self
            .objects
            .get(framebuffer, Kind::Framebuffer)?
            .framebuffer_attachments;
        let entry = if point == DEPTH_STENCIL_ATTACHMENT {
            let depth = entries.get(&gl::DEPTH_ATTACHMENT);
            // GLES3.0 §6.1.13 compares attached objects for this query,
            // not layers/mip images. Completeness still compares images.
            if depth.map(|entry| (entry.id, entry.kind))
                != entries
                    .get(&gl::STENCIL_ATTACHMENT)
                    .map(|entry| (entry.id, entry.kind))
            {
                return Err(gl::INVALID_OPERATION);
            }
            depth
        } else {
            entries.get(&point)
        };
        if pname == gl::FRAMEBUFFER_ATTACHMENT_OBJECT_NAME {
            return Ok(entry.map_or(Value::Null, |entry| json!(entry.id)));
        }
        if pname == gl::FRAMEBUFFER_ATTACHMENT_OBJECT_TYPE {
            return Ok(json!(entry.map_or(gl::NONE, |entry| {
                if entry.kind == Kind::Texture {
                    gl::TEXTURE
                } else {
                    gl::RENDERBUFFER
                }
            })));
        }
        let entry = entry.ok_or(gl::INVALID_OPERATION)?;
        if !valid_pname {
            return Err(gl::INVALID_ENUM);
        }
        if [
            gl::FRAMEBUFFER_ATTACHMENT_TEXTURE_LEVEL,
            gl::FRAMEBUFFER_ATTACHMENT_TEXTURE_CUBE_MAP_FACE,
            0x8cd4,
        ]
        .contains(&pname)
        {
            if entry.kind != Kind::Texture {
                return Err(gl::INVALID_ENUM);
            }
            return Ok(json!(match pname {
                gl::FRAMEBUFFER_ATTACHMENT_TEXTURE_LEVEL => entry.level,
                0x8cd4 => entry.layer.unwrap_or(0),
                _ =>
                    if (0x8515..=0x851a).contains(&entry.target) {
                        entry.target as i32
                    } else {
                        0
                    },
            }));
        }
        if point == DEPTH_STENCIL_ATTACHMENT && pname == 0x8211 {
            return Err(gl::INVALID_OPERATION);
        }
        let mut value = 0;
        unsafe {
            gl::GetFramebufferAttachmentParameteriv(target, point, pname, &mut value);
        }
        self.driver_result()?;
        Ok(json!(value))
    }
}
