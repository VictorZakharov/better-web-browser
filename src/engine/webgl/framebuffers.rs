//! Author framebuffer handles cannot reveal or mutate the private default surface attachments.
use super::{Command, Kind, Result, WebGl, gl, json};
use serde_json::Value;
const DEPTH_STENCIL: u32 = 0x84f9;
impl WebGl {
    pub(super) fn framebuffer_command(&mut self, c: &Command) -> Result<Value> {
        if self.options.api == super::ApiVersion::Two {
            match c.op.as_str() {
                "bindFramebuffer" => return self.core_bind_framebuffer(c),
                "deleteFramebuffer" => return self.core_delete_framebuffer(c),
                "renderbufferStorage" => return self.core_renderbuffer_storage(c),
                "getFramebufferAttachmentParameter" => return self.core_attachment_parameter(c),
                "framebufferTexture2D" | "framebufferRenderbuffer" => {
                    self.core_attach_framebuffer(c)?;
                    return Ok(Value::Null);
                }
                "checkFramebufferStatus" => {
                    return Ok(json!(self.core_framebuffer_status(c.u(0)?)?));
                }
                _ => {}
            }
        }
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
                if id != 0 && self.objects.get(id, Kind::Renderbuffer)?.pending_delete {
                    return Err(gl::INVALID_OPERATION);
                }
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
                    unsafe {
                        gl::BindRenderbuffer(gl::RENDERBUFFER, 0);
                    }
                }
                if !framebuffer {
                    self.detach_current_resource(id, Kind::Renderbuffer)?;
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
                self.driver_result()?;
                self.objects
                    .get_mut(self.renderbuffer, Kind::Renderbuffer)?
                    .renderbuffer_format = format;
            }
            "framebufferTexture2D" | "framebufferRenderbuffer" => {
                self.attach_framebuffer(c)?;
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
                if !([
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
                    || self.options.api == super::ApiVersion::Two && pname == 0x8cab)
                {
                    return Err(gl::INVALID_ENUM);
                }
                let mut value = 0;
                unsafe {
                    gl::GetRenderbufferParameteriv(gl::RENDERBUFFER, pname, &mut value);
                }
                self.driver_result()?;
                return Ok(json!(if self.options.api == super::ApiVersion::One
                    && pname == gl::RENDERBUFFER_INTERNAL_FORMAT
                    && value == 0x88F0
                {
                    DEPTH_STENCIL as i32
                } else {
                    value
                }));
            }
            "getFramebufferAttachmentParameter" => {
                return self.attachment_parameter(c);
            }
            _ => return Err(gl::INVALID_OPERATION),
        }
        self.driver_result()?;
        Ok(Value::Null)
    }
}
