//! Author framebuffer handles cannot reveal or mutate the private default surface attachments.
use super::{Command, Kind, Result, WebGl, gl, json};
use serde_json::Value;
const DEPTH_STENCIL: u32 = 0x84f9;
const DEPTH_STENCIL_ATTACHMENT: u32 = 0x821a;
impl WebGl {
    pub(super) fn framebuffer_command(&mut self, c: &Command) -> Result<Value> {
        match c.op.as_str() {
            "createFramebuffer" | "createRenderbuffer" => {
                let framebuffer = c.op == "createFramebuffer";
                let kind = if framebuffer {
                    Kind::Framebuffer
                } else {
                    Kind::Renderbuffer
                };
                let mut native = 0;
                unsafe {
                    if framebuffer {
                        gl::GenFramebuffers(1, &mut native);
                    } else {
                        gl::GenRenderbuffers(1, &mut native);
                    }
                }
                self.driver_result()?;
                match self.objects.insert(kind, native) {
                    Ok(id) => return Ok(json!(id)),
                    Err(error) => {
                        unsafe {
                            if framebuffer {
                                gl::DeleteFramebuffers(1, &native);
                            } else {
                                gl::DeleteRenderbuffers(1, &native);
                            }
                        }
                        return Err(error);
                    }
                }
            }
            "bindFramebuffer" => {
                if c.u(0)? != gl::FRAMEBUFFER {
                    return Err(gl::INVALID_ENUM);
                }
                let id = c.u(1)?;
                let native = if id == 0 {
                    self.surface.framebuffer
                } else {
                    self.objects.get(id, Kind::Framebuffer)?.native
                };
                unsafe {
                    gl::BindFramebuffer(gl::FRAMEBUFFER, native);
                }
                self.driver_result()?;
                self.framebuffer = id;
            }
            "bindRenderbuffer" => {
                if c.u(0)? != gl::RENDERBUFFER {
                    return Err(gl::INVALID_ENUM);
                }
                let id = c.u(1)?;
                let native = self.objects.name(id, Kind::Renderbuffer)?;
                unsafe {
                    gl::BindRenderbuffer(gl::RENDERBUFFER, native);
                }
                self.driver_result()?;
                self.renderbuffer = id;
            }
            "deleteFramebuffer" | "deleteRenderbuffer" => {
                let framebuffer = c.op == "deleteFramebuffer";
                let id = c.u(0)?;
                if framebuffer && id != 0 && self.framebuffer == id {
                    unsafe {
                        gl::BindFramebuffer(gl::FRAMEBUFFER, self.surface.framebuffer);
                    }
                    self.framebuffer = 0;
                }
                if !framebuffer && self.renderbuffer == id {
                    self.renderbuffer = 0;
                }
                self.objects.delete(
                    id,
                    if framebuffer {
                        Kind::Framebuffer
                    } else {
                        Kind::Renderbuffer
                    },
                )?;
            }
            "renderbufferStorage" => {
                if c.u(0)? != gl::RENDERBUFFER {
                    return Err(gl::INVALID_ENUM);
                }
                let format = c.u(1)?;
                let width = c.n(2)?;
                let height = c.n(3)?;
                let bytes =
                    super::texture_formats::renderbuffer_bytes(format, &self.extensions.textures)?;
                if !(0..=4096).contains(&width) || !(0..=4096).contains(&height) {
                    return Err(gl::INVALID_VALUE);
                }
                self.objects.get(self.renderbuffer, Kind::Renderbuffer)?;
                self.charge(0, width as usize * height as usize * bytes)?;
                unsafe {
                    gl::RenderbufferStorage(
                        gl::RENDERBUFFER,
                        if format == DEPTH_STENCIL {
                            0x88F0
                        } else {
                            format
                        },
                        width,
                        height,
                    );
                }
            }
            "framebufferTexture2D" | "framebufferRenderbuffer" => {
                if c.u(0)? != gl::FRAMEBUFFER {
                    return Err(gl::INVALID_ENUM);
                }
                if self.framebuffer == 0 {
                    return Err(gl::INVALID_OPERATION);
                }
                let attachment = c.u(1)?;
                if ![
                    gl::COLOR_ATTACHMENT0,
                    gl::DEPTH_ATTACHMENT,
                    gl::STENCIL_ATTACHMENT,
                    DEPTH_STENCIL_ATTACHMENT,
                ]
                .contains(&attachment)
                {
                    return Err(gl::INVALID_ENUM);
                }
                let attachments: &[u32] = if attachment == DEPTH_STENCIL_ATTACHMENT {
                    &[gl::DEPTH_ATTACHMENT, gl::STENCIL_ATTACHMENT]
                } else {
                    std::slice::from_ref(&attachment)
                };
                if c.op == "framebufferTexture2D" {
                    let target = c.u(2)?;
                    if target != gl::TEXTURE_2D
                        && !(gl::TEXTURE_CUBE_MAP_POSITIVE_X..=gl::TEXTURE_CUBE_MAP_NEGATIVE_Z)
                            .contains(&target)
                    {
                        return Err(gl::INVALID_ENUM);
                    }
                    if c.n(4)? != 0 {
                        return Err(gl::INVALID_VALUE);
                    }
                    let native = self.objects.name(c.u(3)?, Kind::Texture)?;
                    for &a in attachments {
                        unsafe {
                            gl::FramebufferTexture2D(gl::FRAMEBUFFER, a, target, native, 0);
                        }
                    }
                } else {
                    if c.u(2)? != gl::RENDERBUFFER {
                        return Err(gl::INVALID_ENUM);
                    }
                    let native = self.objects.name(c.u(3)?, Kind::Renderbuffer)?;
                    for &a in attachments {
                        unsafe {
                            gl::FramebufferRenderbuffer(
                                gl::FRAMEBUFFER,
                                a,
                                gl::RENDERBUFFER,
                                native,
                            );
                        }
                    }
                }
            }
            "checkFramebufferStatus" => {
                if c.u(0)? != gl::FRAMEBUFFER {
                    return Err(gl::INVALID_ENUM);
                }
                return Ok(json!(self.framebuffer_status()?));
            }
            "getRenderbufferParameter" => {
                if c.u(0)? != gl::RENDERBUFFER {
                    return Err(gl::INVALID_ENUM);
                }
                self.objects.get(self.renderbuffer, Kind::Renderbuffer)?;
                let pname = c.u(1)?;
                if ![
                    gl::RENDERBUFFER_WIDTH,
                    gl::RENDERBUFFER_HEIGHT,
                    gl::RENDERBUFFER_INTERNAL_FORMAT,
                    gl::RENDERBUFFER_RED_SIZE,
                    gl::RENDERBUFFER_GREEN_SIZE,
                    gl::RENDERBUFFER_BLUE_SIZE,
                    gl::RENDERBUFFER_ALPHA_SIZE,
                    gl::RENDERBUFFER_DEPTH_SIZE,
                    gl::RENDERBUFFER_STENCIL_SIZE,
                ]
                .contains(&pname)
                {
                    return Err(gl::INVALID_ENUM);
                }
                let mut value = 0;
                unsafe {
                    gl::GetRenderbufferParameteriv(gl::RENDERBUFFER, pname, &mut value);
                }
                self.driver_result()?;
                return Ok(json!(
                    if pname == gl::RENDERBUFFER_INTERNAL_FORMAT && value == 0x88F0 {
                        DEPTH_STENCIL as i32
                    } else {
                        value
                    }
                ));
            }
            "getFramebufferAttachmentParameter" => {
                if c.u(0)? != gl::FRAMEBUFFER {
                    return Err(gl::INVALID_ENUM);
                }
                if self.framebuffer == 0 {
                    return Err(gl::INVALID_OPERATION);
                }
                let attachment = c.u(1)?;
                let pname = c.u(2)?;
                let color_types =
                    self.extensions
                        .textures
                        .enabled(super::texture_capabilities::TextureCapability::ColorFloat)
                        || self.extensions.textures.enabled(
                            super::texture_capabilities::TextureCapability::ColorHalfFloat,
                        );
                if attachment == DEPTH_STENCIL_ATTACHMENT
                    && pname == super::texture_formats::COMPONENT_TYPE
                    && color_types
                {
                    return Err(gl::INVALID_OPERATION);
                }
                if ![
                    gl::COLOR_ATTACHMENT0,
                    gl::DEPTH_ATTACHMENT,
                    gl::STENCIL_ATTACHMENT,
                ]
                .contains(&attachment)
                {
                    return Err(gl::INVALID_ENUM);
                }
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
                let mut value = 0;
                unsafe {
                    gl::GetFramebufferAttachmentParameteriv(
                        gl::FRAMEBUFFER,
                        attachment,
                        pname,
                        &mut value,
                    );
                }
                self.driver_result()?;
                if pname == gl::FRAMEBUFFER_ATTACHMENT_OBJECT_NAME {
                    let mut kind = 0;
                    unsafe {
                        gl::GetFramebufferAttachmentParameteriv(
                            gl::FRAMEBUFFER,
                            attachment,
                            gl::FRAMEBUFFER_ATTACHMENT_OBJECT_TYPE,
                            &mut kind,
                        );
                    }
                    return Ok(json!(self.objects.public_name(
                        value as u32,
                        if kind == gl::TEXTURE as i32 {
                            Kind::Texture
                        } else {
                            Kind::Renderbuffer
                        }
                    )));
                }
                return Ok(json!(value));
            }
            _ => return Err(gl::INVALID_OPERATION),
        }
        self.driver_result()?;
        Ok(Value::Null)
    }
}
