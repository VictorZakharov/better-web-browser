//! Uniform output shape comes from linked native reflection, not author IPC.
use super::{Command, Kind, MAX_SHADER_BYTES, Result, WebGl, gl, json};
use serde_json::Value;

impl WebGl {
    pub(super) fn uniform_type(&mut self, program: u32, name: &str) -> Result<u32> {
        let mut count = 0;
        unsafe {
            gl::GetProgramiv(program, gl::ACTIVE_UNIFORMS, &mut count);
        }
        self.driver_result()?;
        if !(0..=16_384).contains(&count) {
            return Err(gl::OUT_OF_MEMORY);
        }
        let wanted = array_family(name);
        let mut bytes = vec![0; MAX_SHADER_BYTES];
        for index in 0..count as u32 {
            let (mut written, mut size, mut kind) = (0, 0, 0);
            unsafe {
                gl::GetActiveUniform(
                    program,
                    index,
                    bytes.len() as i32,
                    &mut written,
                    &mut size,
                    &mut kind,
                    bytes.as_mut_ptr().cast(),
                );
            }
            self.driver_result()?;
            let end = (written.max(0) as usize).min(bytes.len());
            let reflected = String::from_utf8_lossy(&bytes[..end]);
            // A location has already been resolved successfully by GLES. Matching
            // its array family handles scalar arrays and array-of-struct fields
            // without assuming consecutive or implementation-defined locations.
            if array_family(&reflected) == wanted {
                return Ok(kind);
            }
        }
        Err(gl::INVALID_OPERATION)
    }

    pub(super) fn get_uniform(&mut self, c: &Command) -> Result<Value> {
        let id = c.u(0)?;
        let program = self.objects.get(id, Kind::Program)?;
        let uniform = self.objects.get(c.u(1)?, Kind::Uniform)?;
        if uniform.owner != id || uniform.generation != program.generation {
            return Err(gl::INVALID_OPERATION);
        }
        let kind = uniform.uniform_type;
        let count = components(kind)?;
        // GLES writes one element, not the entire declared uniform array.
        // A fixed 16-component allocation covers every WebGL 1 uniform type.
        let mut floats = [0.0f32; 16];
        let mut integers = [0i32; 16];
        let float = [
            gl::FLOAT,
            gl::FLOAT_VEC2,
            gl::FLOAT_VEC3,
            gl::FLOAT_VEC4,
            gl::FLOAT_MAT2,
            gl::FLOAT_MAT3,
            gl::FLOAT_MAT4,
        ]
        .contains(&kind);
        unsafe {
            if float {
                gl::GetUniformfv(program.native, uniform.native as i32, floats.as_mut_ptr());
            } else {
                gl::GetUniformiv(program.native, uniform.native as i32, integers.as_mut_ptr());
            }
        }
        self.driver_result()?;
        let values = if float {
            json!(
                floats[..count]
                    .iter()
                    .copied()
                    .map(super::float_values::encode)
                    .collect::<Vec<_>>()
            )
        } else {
            json!(&integers[..count])
        };
        Ok(json!({"kind":kind,"values":values}))
    }
}

fn components(kind: u32) -> Result<usize> {
    match kind {
        gl::FLOAT | gl::INT | gl::BOOL | gl::SAMPLER_2D | gl::SAMPLER_CUBE => Ok(1),
        gl::FLOAT_VEC2 | gl::INT_VEC2 | gl::BOOL_VEC2 => Ok(2),
        gl::FLOAT_VEC3 | gl::INT_VEC3 | gl::BOOL_VEC3 => Ok(3),
        gl::FLOAT_VEC4 | gl::INT_VEC4 | gl::BOOL_VEC4 | gl::FLOAT_MAT2 => Ok(4),
        gl::FLOAT_MAT3 => Ok(9),
        gl::FLOAT_MAT4 => Ok(16),
        _ => Err(gl::INVALID_ENUM),
    }
}

fn array_family(name: &str) -> String {
    let mut result = String::new();
    let mut rest = name;
    while let Some(start) = rest.find('[') {
        result.push_str(&rest[..start]);
        let Some(end) = rest[start..].find(']') else {
            result.push_str(&rest[start..]);
            return result;
        };
        let index = &rest[start + 1..start + end];
        if !index.is_empty() && index.bytes().all(|b| b.is_ascii_digit()) {
            result.push_str("[]");
        } else {
            result.push_str(&rest[start..=start + end]);
        }
        rest = &rest[start + end + 1..];
    }
    result.push_str(rest);
    // A scalar array's unsuffixed name aliases element zero; an array of structs
    // still keeps its [] before the field separator and cannot alias another field.
    if result.ends_with("[]") {
        result.truncate(result.len() - 2);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn uniform_array_families_preserve_struct_field_identity() {
        assert_eq!(array_family("colors"), array_family("colors[0]"));
        assert_eq!(array_family("colors[0]"), array_family("colors[12]"));
        assert_eq!(
            array_family("lights[0].color"),
            array_family("lights[2].color")
        );
        assert_ne!(
            array_family("lights[0].color"),
            array_family("lights[0].position")
        );
        assert_ne!(
            array_family("lights[0].color"),
            array_family("lights.color")
        );
        assert_eq!(array_family("matrix[1][2]"), array_family("matrix[0][0]"));
        assert_eq!(array_family("bad["), "bad[");
        assert_eq!(array_family("bad[x]"), "bad[x]");
        assert_eq!(components(gl::FLOAT_MAT4), Ok(16));
        assert_eq!(components(gl::BOOL_VEC3), Ok(3));
        assert_eq!(components(u32::MAX), Err(gl::INVALID_ENUM));
    }
}
