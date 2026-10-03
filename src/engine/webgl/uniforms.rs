//! Uniform names retain both their owning program and its link generation.
use super::{Command, Kind, Result, WebGl, gl, json};
use serde_json::Value;
use std::ffi::CString;

impl WebGl {
    pub(super) fn uniform_command(&mut self, c: &Command) -> Result<Value> {
        if c.op == "getUniformLocation" {
            let owner = c.u(0)?;
            let object = self.objects.get(owner, Kind::Program)?;
            if c.text.len() > 256 || c.text.starts_with("gl_") {
                return Err(gl::INVALID_VALUE);
            }
            let mut linked = 0;
            unsafe {
                gl::GetProgramiv(object.native, gl::LINK_STATUS, &mut linked);
            }
            if linked == 0 {
                return Err(gl::INVALID_OPERATION);
            }
            let name = CString::new(c.text.as_str()).map_err(|_| gl::INVALID_VALUE)?;
            let location = unsafe { gl::GetUniformLocation(object.native, name.as_ptr()) };
            let generation = object.generation;
            self.driver_result()?;
            if location < 0 {
                return Ok(Value::Null);
            }
            if let Some(id) = self.objects.uniform(owner, generation, location as u32) {
                return Ok(json!(id));
            }
            let id = self.objects.insert(Kind::Uniform, location as u32)?;
            let location = self.objects.get_mut(id, Kind::Uniform)?;
            location.owner = owner;
            location.generation = generation;
            return Ok(json!(id));
        }
        let id = c.u(0)?;
        // A null location is a specified no-op, not an invalid program/location error.
        if id == 0 {
            return Ok(Value::Null);
        }
        let object = self.objects.get(id, Kind::Uniform)?;
        let program = self.objects.get(object.owner, Kind::Program)?;
        if self.program != object.owner || program.generation != object.generation {
            return Err(gl::INVALID_OPERATION);
        }
        let location = object.native as i32;
        let floats =
            c.f.iter()
                .enumerate()
                .map(|(index, _)| c.float(index))
                .collect::<Result<Vec<_>>>()?;
        let integers =
            c.i.iter()
                .skip(1)
                .map(|value| i32::try_from(*value).map_err(|_| gl::INVALID_VALUE))
                .collect::<Result<Vec<_>>>()?;
        let matrix = c.op.starts_with("uniformMatrix");
        let columns =
            c.op.as_bytes()
                .get(if matrix { 13 } else { 7 })
                .copied()
                .ok_or(gl::INVALID_OPERATION)?;
        let columns = usize::from(columns.checked_sub(b'0').ok_or(gl::INVALID_OPERATION)?);
        if !(1..=4).contains(&columns) {
            return Err(gl::INVALID_OPERATION);
        }
        let integer = c.op.ends_with('i') || c.op.ends_with("iv");
        let values = if integer {
            integers.len()
        } else {
            floats.len()
        };
        let group = if matrix { columns * columns } else { columns };
        if !values.is_multiple_of(group) || (!c.op.ends_with('v') && values != group) {
            return Err(gl::INVALID_VALUE);
        }
        if matrix && c.u(1)? != 0 {
            return Err(gl::INVALID_VALUE);
        }
        let count = i32::try_from(values / group).map_err(|_| gl::INVALID_VALUE)?;
        // SAFETY: private typed location, generation/current-program checks, and initialized
        // vectors sized to count*components. GLES validates the declared uniform type.
        unsafe {
            match c.op.as_str() {
                "uniform1f" | "uniform1fv" => gl::Uniform1fv(location, count, floats.as_ptr()),
                "uniform2f" | "uniform2fv" => gl::Uniform2fv(location, count, floats.as_ptr()),
                "uniform3f" | "uniform3fv" => gl::Uniform3fv(location, count, floats.as_ptr()),
                "uniform4f" | "uniform4fv" => gl::Uniform4fv(location, count, floats.as_ptr()),
                "uniform1i" | "uniform1iv" => gl::Uniform1iv(location, count, integers.as_ptr()),
                "uniform2i" | "uniform2iv" => gl::Uniform2iv(location, count, integers.as_ptr()),
                "uniform3i" | "uniform3iv" => gl::Uniform3iv(location, count, integers.as_ptr()),
                "uniform4i" | "uniform4iv" => gl::Uniform4iv(location, count, integers.as_ptr()),
                "uniformMatrix2fv" => gl::UniformMatrix2fv(location, count, 0, floats.as_ptr()),
                "uniformMatrix3fv" => gl::UniformMatrix3fv(location, count, 0, floats.as_ptr()),
                "uniformMatrix4fv" => gl::UniformMatrix4fv(location, count, 0, floats.as_ptr()),
                _ => return Err(gl::INVALID_OPERATION),
            }
        }
        self.driver_result()?;
        Ok(Value::Null)
    }
}
