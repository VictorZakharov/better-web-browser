//! Uniform output shape comes from linked native reflection, not author IPC.
use super::{Command, Kind, Result, WebGl, gl, json};
use serde_json::Value;

impl WebGl {
    pub(super) fn get_uniform(&mut self, c: &Command) -> Result<Value> {
        let id = c.u(0)?;
        let program = self.objects.get(id, Kind::Program)?;
        let uniform = self.objects.uniform_location(c.u(1)?)?;
        if uniform.owner != id || uniform.generation != program.generation {
            return Err(gl::INVALID_OPERATION);
        }
        let kind = uniform.uniform_type;
        let count = if self.options.api == super::ApiVersion::Two {
            super::core_uniforms::components(kind)
                .map(Ok)
                .unwrap_or_else(|| components(kind))?
        } else {
            components(kind)?
        };
        if self.options.api == super::ApiVersion::Two && super::core_uniforms::is_unsigned(kind) {
            let mut values = [0u32; 4];
            let entry = self
                .core
                .as_ref()
                .ok_or(gl::INVALID_OPERATION)?
                .get_unsigned_uniform;
            // SAFETY: one reflected uint uniform element, at most four components;
            // typed ownership and generation were checked before accessing its native location.
            unsafe {
                entry(program.native, uniform.native as i32, values.as_mut_ptr());
            }
            self.driver_result()?;
            return Ok(json!({"kind":kind,"values":&values[..count]}));
        }
        // GLES writes one element, not the entire declared uniform array.
        // A fixed 16-component allocation covers every WebGL 1 uniform type.
        let mut floats = [0.0f32; 16];
        let mut integers = [0i32; 16];
        let float = super::core_uniforms::MATRIX_TYPES.contains(&kind)
            || [
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

pub(super) fn array_family(name: &str) -> String {
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
