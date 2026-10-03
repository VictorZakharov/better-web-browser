//! Reflection reports logical WebGL attachment identity, never a private GLES3
//! alias or a driver object name. Retained deleted resources keep their identity.
use super::framebuffer_attachments::DEPTH_STENCIL_ATTACHMENT;
use super::texture_capabilities::TextureCapability;
use super::{Command, Kind, Result, WebGl, gl, json};
use serde_json::Value;

impl WebGl {
    pub(super) fn attachment_parameter(&self, c: &Command) -> Result<Value> {
        if c.u(0)? != gl::FRAMEBUFFER {
            return Err(gl::INVALID_ENUM);
        }
        if self.framebuffer == 0 {
            return Err(gl::INVALID_OPERATION);
        }
        let point = c.u(1)?;
        let pname = c.u(2)?;
        if ![
            gl::COLOR_ATTACHMENT0,
            gl::DEPTH_ATTACHMENT,
            gl::STENCIL_ATTACHMENT,
            DEPTH_STENCIL_ATTACHMENT,
        ]
        .contains(&point)
        {
            return Err(gl::INVALID_ENUM);
        }
        let color_types = self
            .extensions
            .textures
            .enabled(TextureCapability::ColorFloat)
            || self
                .extensions
                .textures
                .enabled(TextureCapability::ColorHalfFloat);
        if !([
            gl::FRAMEBUFFER_ATTACHMENT_OBJECT_TYPE,
            gl::FRAMEBUFFER_ATTACHMENT_OBJECT_NAME,
            gl::FRAMEBUFFER_ATTACHMENT_TEXTURE_LEVEL,
            gl::FRAMEBUFFER_ATTACHMENT_TEXTURE_CUBE_MAP_FACE,
        ]
        .contains(&pname)
            || pname == super::texture_formats::COMPONENT_TYPE && color_types)
        {
            return Err(gl::INVALID_ENUM);
        }
        if pname == super::texture_formats::COMPONENT_TYPE && point == DEPTH_STENCIL_ATTACHMENT {
            return Err(gl::INVALID_OPERATION);
        }
        let entries = &self
            .objects
            .get(self.framebuffer, Kind::Framebuffer)?
            .framebuffer_attachments;
        let entry = entries.get(&point).or_else(|| {
            [gl::DEPTH_ATTACHMENT, gl::STENCIL_ATTACHMENT]
                .contains(&point)
                .then(|| entries.get(&DEPTH_STENCIL_ATTACHMENT))
                .flatten()
        });
        let Some(entry) = entry else {
            return match pname {
                gl::FRAMEBUFFER_ATTACHMENT_OBJECT_TYPE => Ok(json!(gl::NONE)),
                gl::FRAMEBUFFER_ATTACHMENT_OBJECT_NAME => Ok(Value::Null),
                _ => Err(gl::INVALID_OPERATION),
            };
        };
        match pname {
            gl::FRAMEBUFFER_ATTACHMENT_OBJECT_TYPE => Ok(json!(if entry.kind == Kind::Texture {
                gl::TEXTURE
            } else {
                gl::RENDERBUFFER
            })),
            gl::FRAMEBUFFER_ATTACHMENT_OBJECT_NAME => Ok(json!(entry.id)),
            gl::FRAMEBUFFER_ATTACHMENT_TEXTURE_LEVEL
            | gl::FRAMEBUFFER_ATTACHMENT_TEXTURE_CUBE_MAP_FACE => {
                if entry.kind != Kind::Texture {
                    return Err(gl::INVALID_ENUM);
                }
                Ok(json!(if pname == gl::FRAMEBUFFER_ATTACHMENT_TEXTURE_LEVEL
                    || entry.target == gl::TEXTURE_2D
                {
                    0
                } else {
                    entry.target
                }))
            }
            _ => {
                let object = self.objects.get(entry.id, entry.kind)?;
                let floating = if entry.kind == Kind::Texture {
                    object
                        .texture_images
                        .get(&(entry.target, 0))
                        .is_some_and(|(_, kind)| {
                            [gl::FLOAT, super::texture_formats::HALF_FLOAT].contains(kind)
                        })
                } else {
                    [
                        super::texture_formats::RGBA32F,
                        super::texture_formats::RGBA16F,
                        super::texture_formats::RGB16F,
                    ]
                    .contains(&object.renderbuffer_format)
                };
                Ok(json!(if floating {
                    gl::FLOAT
                } else if point == gl::STENCIL_ATTACHMENT {
                    gl::UNSIGNED_INT
                } else {
                    0x8c17u32 /* UNSIGNED_NORMALIZED_EXT */
                }))
            }
        }
    }
}
