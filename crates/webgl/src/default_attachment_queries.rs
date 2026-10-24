//! Reflect actual private storage without exposing its texture/renderbuffer names.
//! GLES3 §6.1.13 uses BACK, DEPTH, STENCIL for the logical default framebuffer.
use super::{Command, Result, WebGl, gl, json};
use serde_json::Value;
impl WebGl {
    pub(super) fn default_attachment_parameter(&mut self, c: &Command) -> Result<Value> {
        let target = c.u(0)?;
        let point = c.u(1)?;
        let pname = c.u(2)?;
        if ![
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
        .contains(&pname)
        {
            return Err(gl::INVALID_ENUM);
        }
        let (native, exists) = match point {
            gl::BACK => (gl::COLOR_ATTACHMENT0, true),
            0x1801 => (gl::DEPTH_ATTACHMENT, self.options.depth),
            0x1802 => (gl::STENCIL_ATTACHMENT, self.options.stencil),
            gl::DEPTH_ATTACHMENT | gl::STENCIL_ATTACHMENT | 0x821a => {
                return Err(gl::INVALID_OPERATION);
            }
            point if self.color_attachment_allowed(point) => return Err(gl::INVALID_OPERATION),
            _ => return Err(gl::INVALID_ENUM),
        };
        if pname == gl::FRAMEBUFFER_ATTACHMENT_OBJECT_TYPE {
            return Ok(json!(if exists { 0x8218 } else { gl::NONE }));
        }
        if !exists {
            return if pname == gl::FRAMEBUFFER_ATTACHMENT_OBJECT_NAME {
                Ok(Value::Null)
            } else {
                Err(gl::INVALID_OPERATION)
            };
        }
        // Default storage is not an author texture or renderbuffer. Neither a
        // native object name nor an invented browser handle may escape here.
        if [
            gl::FRAMEBUFFER_ATTACHMENT_OBJECT_NAME,
            gl::FRAMEBUFFER_ATTACHMENT_TEXTURE_LEVEL,
            gl::FRAMEBUFFER_ATTACHMENT_TEXTURE_CUBE_MAP_FACE,
            0x8cd4,
        ]
        .contains(&pname)
        {
            return Err(gl::INVALID_ENUM);
        }
        let mut value = 0;
        unsafe { gl::GetFramebufferAttachmentParameteriv(target, native, pname, &mut value) };
        self.driver_result()?;
        Ok(json!(value))
    }
}
