//! Link-time capture names remain owned C strings; reflection sizes are bounded.
use super::{ApiVersion, Command, Kind, Result, WebGl, gl};
use serde_json::{Value, json};
use std::ffi::CString;
impl WebGl {
    pub(super) fn transform_varyings(&mut self, c: &Command) -> Result<Value> {
        if self.options.api != ApiVersion::Two {
            return Err(gl::INVALID_OPERATION);
        }
        let native = self.objects.get(c.u(0)?, Kind::Program)?.native;
        if c.op == "transformFeedbackVaryings" {
            let mode = c.u(1)?;
            if ![0x8c8c, 0x8c8d].contains(&mode) {
                return Err(gl::INVALID_ENUM);
            }
            let names: Vec<String> =
                serde_json::from_str(&c.text).map_err(|_| gl::INVALID_VALUE)?;
            if names.len() > 64 || c.text.len() > super::MAX_SHADER_BYTES {
                return Err(gl::INVALID_VALUE);
            }
            if mode == 0x8c8d && names.len() > self.transform_feedback.count {
                return Err(gl::INVALID_VALUE);
            }
            let names = names
                .into_iter()
                .map(|name| {
                    // Capture names include built-in vertex outputs such as
                    // gl_Position. ANGLE validates their link-time availability;
                    // the reserved identifier rule for author declarations does
                    // not prohibit selecting an existing built-in output.
                    if name.len() > 1024 || !name.is_ascii() {
                        return Err(gl::INVALID_VALUE);
                    }
                    CString::new(name).map_err(|_| gl::INVALID_VALUE)
                })
                .collect::<Result<Vec<_>>>()?;
            let pointers = names.iter().map(|name| name.as_ptr()).collect::<Vec<_>>();
            unsafe {
                (self.core.as_ref().unwrap().transform.varyings)(
                    native,
                    pointers.len() as i32,
                    pointers.as_ptr(),
                    mode,
                )
            };
            self.driver_result()?;
            return Ok(Value::Null);
        }
        let index = c.u(1)?;
        let mut count = 0;
        let mut name_size = 0;
        unsafe {
            gl::GetProgramiv(native, 0x8c83, &mut count);
            gl::GetProgramiv(native, 0x8c76, &mut name_size);
        }
        self.driver_result()?;
        if index >= count.max(0) as u32 {
            return Err(gl::INVALID_VALUE);
        }
        if !(1..=1025).contains(&name_size) {
            return Err(gl::INVALID_OPERATION);
        }
        let mut name = vec![0u8; name_size as usize];
        let (mut length, mut size, mut kind) = (0, 0, 0);
        unsafe {
            (self.core.as_ref().unwrap().transform.varying)(
                native,
                index,
                name_size,
                &mut length,
                &mut size,
                &mut kind,
                name.as_mut_ptr().cast(),
            )
        };
        self.driver_result()?;
        if length < 0 || length >= name_size || size < 1 {
            return Err(gl::INVALID_OPERATION);
        }
        name.truncate(length as usize);
        Ok(json!({"name":String::from_utf8_lossy(&name), "size":size, "type":kind}))
    }
}
