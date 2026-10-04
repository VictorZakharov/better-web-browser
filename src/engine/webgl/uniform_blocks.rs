//! Program-owned uniform block reflection; no author-sized native output writes.
use super::{ApiVersion, Command, Kind, Result, WebGl, gl};
use serde_json::{Value, json};
use std::ffi::CString;

pub(super) fn name(value: &str) -> Result<CString> {
    if value.len() > 1024 {
        return Err(gl::INVALID_VALUE);
    }
    CString::new(value).map_err(|_| gl::INVALID_VALUE)
}
pub(super) fn program_count(program: u32, pname: u32) -> Result<u32> {
    let mut count = 0;
    unsafe { gl::GetProgramiv(program, pname, &mut count) };
    if !(0..=4096).contains(&count) {
        return Err(gl::OUT_OF_MEMORY);
    }
    Ok(count as u32)
}

impl WebGl {
    pub(super) fn uniform_block_command(&mut self, c: &Command) -> Result<Value> {
        if self.options.api != ApiVersion::Two {
            return Err(gl::INVALID_OPERATION);
        }
        let program = self.objects.get(c.u(0)?, Kind::Program)?.native;
        let core = self.core.as_ref().ok_or(gl::INVALID_OPERATION)?;
        match c.op.as_str() {
            "getFragDataLocation" => {
                let name = name(&c.text)?;
                let location = unsafe { (core.frag_data_location)(program, name.as_ptr()) };
                self.driver_result()?;
                Ok(json!(location))
            }
            "getUniformIndices" => {
                let names: Vec<String> =
                    serde_json::from_str(&c.text).map_err(|_| gl::INVALID_VALUE)?;
                if names.len() > 4096 {
                    return Err(gl::INVALID_VALUE);
                }
                let names = names
                    .iter()
                    .map(|value| name(value))
                    .collect::<Result<Vec<_>>>()?;
                let pointers: Vec<_> = names.iter().map(|name| name.as_ptr()).collect();
                let mut indices = vec![u32::MAX; names.len()];
                if !names.is_empty() {
                    unsafe {
                        (core.uniform_indices)(
                            program,
                            names.len() as i32,
                            pointers.as_ptr(),
                            indices.as_mut_ptr(),
                        )
                    };
                }
                self.driver_result()?;
                Ok(json!(indices))
            }
            "getUniformBlockIndex" => {
                let name = name(&c.text)?;
                let index = unsafe { (core.uniform_block_index)(program, name.as_ptr()) };
                self.driver_result()?;
                Ok(json!(index))
            }
            "getActiveUniforms" => self.active_uniform_properties(c, program),
            "getActiveUniformBlockParameter" => self.uniform_block_property(c, program),
            "getActiveUniformBlockName" | "uniformBlockBinding" => {
                let index = c.u(1)?;
                if index >= program_count(program, 0x8a36)? {
                    return Err(gl::INVALID_VALUE);
                }
                if c.op == "uniformBlockBinding" {
                    let binding = c.u(2)?;
                    if binding as usize >= self.indexed_uniforms.0.len() {
                        return Err(gl::INVALID_VALUE);
                    }
                    unsafe { (core.uniform_block_binding)(program, index, binding) };
                    self.driver_result()?;
                    return Ok(Value::Null);
                }
                let mut size = 0;
                unsafe { (core.uniform_block_query)(program, index, 0x8a41, &mut size) };
                self.driver_result()?;
                if !(1..=1025).contains(&size) {
                    return Err(gl::OUT_OF_MEMORY);
                }
                let mut bytes = vec![0u8; size as usize];
                let mut written = 0;
                unsafe {
                    (self.core.as_ref().unwrap().uniform_block_name)(
                        program,
                        index,
                        size,
                        &mut written,
                        bytes.as_mut_ptr().cast(),
                    )
                };
                self.driver_result()?;
                if written < 0 || written >= size {
                    return Err(gl::INVALID_OPERATION);
                }
                bytes.truncate(written as usize);
                Ok(json!(
                    String::from_utf8(bytes).map_err(|_| gl::INVALID_OPERATION)?
                ))
            }
            _ => Err(gl::INVALID_OPERATION),
        }
    }
}
