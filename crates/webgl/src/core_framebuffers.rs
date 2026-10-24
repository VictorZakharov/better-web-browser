//! Logical WebGL2 read/draw framebuffer identities remain independent.
use super::framebuffer_guard::{DRAW, READ, READ_BINDING};
use super::{ApiVersion, Command, Kind, Result, WebGl, gl, json};
use serde_json::Value;

impl WebGl {
    pub(super) fn core_framebuffer_id(&self, target: u32) -> Result<u32> {
        match target {
            gl::FRAMEBUFFER | DRAW => Ok(self.framebuffer),
            READ => Ok(self.read_framebuffer),
            _ => Err(gl::INVALID_ENUM),
        }
    }
    pub(super) fn core_bind_framebuffer(&mut self, c: &Command) -> Result<Value> {
        let target = c.u(0)?;
        self.core_framebuffer_id(target)?;
        let id = c.u(1)?;
        let native = if id == 0 {
            self.surface.framebuffer
        } else {
            self.objects.get(id, Kind::Framebuffer)?.native
        };
        unsafe {
            gl::BindFramebuffer(target, native);
        }
        self.driver_result()?;
        if target != READ {
            self.framebuffer = id;
        }
        if target != DRAW {
            self.read_framebuffer = id;
        }
        Ok(Value::Null)
    }
    pub(super) fn core_delete_framebuffer(&mut self, c: &Command) -> Result<Value> {
        let id = c.u(0)?;
        if id == 0 {
            return Ok(Value::Null);
        }
        self.objects.get(id, Kind::Framebuffer)?;
        if self.framebuffer == id {
            unsafe {
                gl::BindFramebuffer(DRAW, self.surface.framebuffer);
            }
            self.framebuffer = 0;
        }
        if self.read_framebuffer == id {
            unsafe {
                gl::BindFramebuffer(READ, self.surface.framebuffer);
            }
            self.read_framebuffer = 0;
        }
        self.objects.delete(id, Kind::Framebuffer)?;
        self.driver_result()?;
        Ok(Value::Null)
    }
    pub(super) fn core_framebuffer_status(&self, target: u32) -> Result<u32> {
        let framebuffer = self.core_framebuffer_id(target)?;
        if framebuffer != 0 {
            for entry in self
                .objects
                .get(framebuffer, Kind::Framebuffer)?
                .framebuffer_attachments
                .values()
            {
                if entry.kind == Kind::Texture {
                    let texture = self.objects.get(entry.id, Kind::Texture)?;
                    if texture.immutable_levels != 0
                        && entry.level as u32 >= texture.immutable_levels
                    {
                        return Ok(gl::FRAMEBUFFER_INCOMPLETE_ATTACHMENT);
                    }
                }
            }
        }
        // ANGLE's WebGL2 compatibility validator enforces native format and
        // depth/stencil-image constraints. This path must not inherit WebGL1's
        // legacy-format whitelist or its three independent depth/stencil slots.
        Ok(unsafe { gl::CheckFramebufferStatus(target) })
    }
    pub(super) fn validate_read_framebuffer(&self) -> Result<()> {
        if self.core_framebuffer_status(READ)? == gl::FRAMEBUFFER_COMPLETE {
            Ok(())
        } else {
            Err(gl::INVALID_FRAMEBUFFER_OPERATION)
        }
    }
    pub(super) fn core_read_buffer(&mut self, c: &Command) -> Result<Value> {
        if self.options.api != ApiVersion::Two {
            return Err(gl::INVALID_OPERATION);
        }
        let source = c.u(0)?;
        // GLES3.0 §4.3.1 distinguishes an unknown enum from a recognized
        // read selector which is incompatible with this framebuffer.
        if source != gl::NONE
            && source != gl::BACK
            && !(gl::COLOR_ATTACHMENT0..=gl::COLOR_ATTACHMENT0 + 31).contains(&source)
        {
            return Err(gl::INVALID_ENUM);
        }
        let native = if self.read_framebuffer == 0 {
            if ![gl::BACK, gl::NONE].contains(&source) {
                return Err(gl::INVALID_OPERATION);
            }
            if source == gl::BACK {
                gl::COLOR_ATTACHMENT0
            } else {
                gl::NONE
            }
        } else {
            if source != gl::NONE && !self.color_attachment_allowed(source) {
                return Err(gl::INVALID_OPERATION);
            }
            source
        };
        unsafe {
            (self.core.as_ref().ok_or(gl::INVALID_OPERATION)?.read_buffer)(native);
        }
        self.driver_result()?;
        if self.read_framebuffer == 0 {
            self.default_read_buffer = source;
        } else {
            self.objects
                .get_mut(self.read_framebuffer, Kind::Framebuffer)?
                .read_buffer = source;
        }
        Ok(Value::Null)
    }
    pub(super) fn core_framebuffer_parameter(&self, pname: u32) -> Result<Option<Value>> {
        if ![READ_BINDING, 0x0c02].contains(&pname) {
            return Ok(None);
        }
        if self.options.api != ApiVersion::Two {
            return Err(gl::INVALID_ENUM);
        }
        Ok(Some(if pname == READ_BINDING {
            if self.read_framebuffer == 0 {
                Value::Null
            } else {
                json!(self.read_framebuffer)
            }
        } else {
            json!(if self.read_framebuffer == 0 {
                self.default_read_buffer
            } else {
                self.objects
                    .get(self.read_framebuffer, Kind::Framebuffer)?
                    .read_buffer
            })
        }))
    }
}
