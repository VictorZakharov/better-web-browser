//! Driver log readback is independently bounded even after compiler failure.
use super::{Command, Kind, MAX_SHADER_BYTES, Result, WebGl, gl, json};
use serde_json::Value;
impl WebGl {
    pub(super) fn shader_log(&mut self, c: &Command) -> Result<Value> {
        let shader = c.op == "getShaderInfoLog";
        let object = self
            .objects
            .get(c.u(0)?, if shader { Kind::Shader } else { Kind::Program })?;
        if shader && !object.shader_log.is_empty() {
            return Ok(json!(object.shader_log));
        }
        let mut length = 0;
        unsafe {
            if shader {
                gl::GetShaderiv(object.native, gl::INFO_LOG_LENGTH, &mut length);
            } else {
                gl::GetProgramiv(object.native, gl::INFO_LOG_LENGTH, &mut length);
            }
        }
        let mut data = vec![0u8; length.clamp(1, MAX_SHADER_BYTES as i32) as usize];
        let mut written = 0;
        unsafe {
            if shader {
                gl::GetShaderInfoLog(
                    object.native,
                    data.len() as i32,
                    &mut written,
                    data.as_mut_ptr().cast(),
                );
            } else {
                gl::GetProgramInfoLog(
                    object.native,
                    data.len() as i32,
                    &mut written,
                    data.as_mut_ptr().cast(),
                );
            }
        }
        data.truncate(written.max(0) as usize);
        self.driver_result()?;
        Ok(json!(String::from_utf8_lossy(&data)))
    }
}
