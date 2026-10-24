//! Uniform/block vectors have independently validated native result lengths.
use super::uniform_blocks::program_count;
use super::{Command, Result, WebGl, gl};
use serde_json::{Value, json};

impl WebGl {
    pub(super) fn active_uniform_properties(&mut self, c: &Command, program: u32) -> Result<Value> {
        let indices: Vec<u32> = serde_json::from_str(&c.text).map_err(|_| gl::INVALID_VALUE)?;
        let pname = c.u(1)?;
        if ![0x8a37, 0x8a38, 0x8a3a, 0x8a3b, 0x8a3c, 0x8a3d, 0x8a3e].contains(&pname) {
            return Err(gl::INVALID_ENUM);
        }
        let count = program_count(program, gl::ACTIVE_UNIFORMS)?;
        if indices.len() > 4096 || indices.iter().any(|index| *index >= count) {
            return Err(gl::INVALID_VALUE);
        }
        let mut values = vec![0; indices.len()];
        if !indices.is_empty() {
            unsafe {
                (self
                    .core
                    .as_ref()
                    .ok_or(gl::INVALID_OPERATION)?
                    .active_uniforms)(
                    program,
                    indices.len() as i32,
                    indices.as_ptr(),
                    pname,
                    values.as_mut_ptr(),
                )
            };
        }
        self.driver_result()?;
        Ok(if pname == 0x8a3e {
            json!(values.iter().map(|value| *value != 0).collect::<Vec<_>>())
        } else {
            json!(values)
        })
    }

    pub(super) fn uniform_block_property(&mut self, c: &Command, program: u32) -> Result<Value> {
        let index = c.u(1)?;
        let pname = c.u(2)?;
        if ![0x8a3f, 0x8a40, 0x8a42, 0x8a43, 0x8a44, 0x8a46].contains(&pname) {
            return Err(gl::INVALID_ENUM);
        }
        if index >= program_count(program, 0x8a36)? {
            return Err(gl::INVALID_VALUE);
        }
        let function = self
            .core
            .as_ref()
            .ok_or(gl::INVALID_OPERATION)?
            .uniform_block_query;
        if pname == 0x8a43 {
            let mut count = 0;
            unsafe { function(program, index, 0x8a42, &mut count) };
            self.driver_result()?;
            let uniforms = program_count(program, gl::ACTIVE_UNIFORMS)?;
            if count < 0 || count as u32 > uniforms {
                return Err(gl::INVALID_OPERATION);
            }
            let mut values = vec![0; count as usize];
            if count != 0 {
                unsafe { function(program, index, pname, values.as_mut_ptr()) };
            }
            self.driver_result()?;
            if values
                .iter()
                .any(|value| *value < 0 || *value as u32 >= uniforms)
            {
                return Err(gl::INVALID_OPERATION);
            }
            return Ok(json!(values));
        }
        let mut value = 0;
        unsafe { function(program, index, pname, &mut value) };
        self.driver_result()?;
        Ok(if [0x8a44, 0x8a46].contains(&pname) {
            json!(value != 0)
        } else {
            json!(value)
        })
    }
}
