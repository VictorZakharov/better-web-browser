//! Public object predicates are backed by the actual driver object lifecycle.
use super::{Command, Kind, Result, WebGl, gl, json};
use serde_json::Value;
impl WebGl {
    pub(super) fn object_query(&mut self, c: &Command) -> Result<Value> {
        let kind = match c.op.as_str() {
            "isBuffer" => Kind::Buffer,
            "isTexture" => Kind::Texture,
            "isFramebuffer" => Kind::Framebuffer,
            "isRenderbuffer" => Kind::Renderbuffer,
            "isShader" => Kind::Shader,
            _ => Kind::Program,
        };
        let Some(native) = self.objects.get(c.u(0)?, kind).ok().map(|o| o.native) else {
            return Ok(json!(false));
        };
        let value = unsafe {
            match kind {
                Kind::Buffer => gl::IsBuffer(native),
                Kind::Texture => gl::IsTexture(native),
                Kind::Framebuffer => gl::IsFramebuffer(native),
                Kind::Renderbuffer => gl::IsRenderbuffer(native),
                Kind::Shader => gl::IsShader(native),
                Kind::Program => gl::IsProgram(native),
                Kind::Uniform => 0,
            }
        };
        Ok(json!(value != 0))
    }
}
