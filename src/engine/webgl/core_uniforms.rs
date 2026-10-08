//! WebGL2 uint vectors and non-square matrices retain typed program-generation ownership.
use super::{ApiVersion, Command, Kind, Result, WebGl, gl};
use serde_json::Value;

pub(super) const UNSIGNED_INT_VEC2: u32 = 0x8dc6;
pub(super) const UNSIGNED_INT_VEC3: u32 = 0x8dc7;
pub(super) const UNSIGNED_INT_VEC4: u32 = 0x8dc8;
pub(super) const MATRIX_TYPES: [u32; 6] = [0x8b65, 0x8b66, 0x8b67, 0x8b68, 0x8b69, 0x8b6a];
pub(super) const MATRIX_COMPONENTS: [usize; 6] = [6, 8, 6, 12, 8, 12];
const MATRIX_OPERATIONS: [&str; 6] = [
    "uniformMatrix2x3fv",
    "uniformMatrix2x4fv",
    "uniformMatrix3x2fv",
    "uniformMatrix3x4fv",
    "uniformMatrix4x2fv",
    "uniformMatrix4x3fv",
];

pub(super) fn is_unsigned(kind: u32) -> bool {
    [
        gl::UNSIGNED_INT,
        UNSIGNED_INT_VEC2,
        UNSIGNED_INT_VEC3,
        UNSIGNED_INT_VEC4,
    ]
    .contains(&kind)
}

pub(super) fn components(kind: u32) -> Option<usize> {
    match kind {
        gl::UNSIGNED_INT => Some(1),
        UNSIGNED_INT_VEC2 => Some(2),
        UNSIGNED_INT_VEC3 => Some(3),
        UNSIGNED_INT_VEC4 => Some(4),
        // All integer/shadow/array/3D samplers store a single texture-unit integer.
        0x8b5f
        | 0x8b62
        | 0x8dc1
        | 0x8dc4
        | 0x8dc5
        | 0x8dca..=0x8dcc
        | 0x8dcf
        | 0x8dd2..=0x8dd4
        | 0x8dd7 => Some(1),
        _ => MATRIX_TYPES
            .iter()
            .position(|value| *value == kind)
            .map(|index| MATRIX_COMPONENTS[index]),
    }
}

impl WebGl {
    pub(super) fn core_uniform_command(&mut self, command: &Command) -> Result<Value> {
        if self.options.api != ApiVersion::Two {
            return Err(gl::INVALID_OPERATION);
        }
        let id = command.u(0)?;
        if id == 0 {
            return Ok(Value::Null);
        }
        let uniform = self.objects.uniform_location(id)?;
        let program = self.objects.get(uniform.owner, Kind::Program)?;
        if self.program != uniform.owner || program.generation != uniform.generation {
            return Err(gl::INVALID_OPERATION);
        }
        let location = uniform.native as i32;
        let core = self.core.as_ref().ok_or(gl::INVALID_OPERATION)?;
        if let Some(index) = MATRIX_OPERATIONS.iter().position(|op| *op == command.op) {
            if command.u(1)? != 0 {
                return Err(gl::INVALID_VALUE);
            }
            let group = MATRIX_COMPONENTS[index];
            if !command.f.len().is_multiple_of(group) {
                return Err(gl::INVALID_VALUE);
            }
            let count = i32::try_from(command.f.len() / group).map_err(|_| gl::INVALID_VALUE)?;
            let values = command
                .f
                .iter()
                .map(|value| *value as f32)
                .collect::<Vec<_>>();
            // SAFETY: fixed operation, current-program typed location, initialized
            // values of exactly count*columns*rows; transpose is false in WebGL.
            unsafe {
                (core.matrix_uniform[index])(location, count, 0, values.as_ptr());
            }
        } else {
            let index = match command.op.as_str() {
                "uniform1ui" | "uniform1uiv" => 0,
                "uniform2ui" | "uniform2uiv" => 1,
                "uniform3ui" | "uniform3uiv" => 2,
                "uniform4ui" | "uniform4uiv" => 3,
                _ => return Err(gl::INVALID_OPERATION),
            };
            let group = index + 1;
            let values = command
                .i
                .iter()
                .skip(1)
                .map(|value| u32::try_from(*value).map_err(|_| gl::INVALID_VALUE))
                .collect::<Result<Vec<_>>>()?;
            if !values.len().is_multiple_of(group)
                || (!command.op.ends_with('v') && values.len() != group)
            {
                return Err(gl::INVALID_VALUE);
            }
            let count = i32::try_from(values.len() / group).map_err(|_| gl::INVALID_VALUE)?;
            // SAFETY: native validation enforces the linked declaration's type;
            // vector length matches the only whitelisted scalar/vector operation.
            unsafe {
                (core.unsigned_uniform[index])(location, count, values.as_ptr());
            }
        }
        self.driver_result()?;
        Ok(Value::Null)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn uniform_components_follow_khronos_types_not_enum_arithmetic() {
        for (kind, count) in MATRIX_TYPES.into_iter().zip(MATRIX_COMPONENTS) {
            assert_eq!(components(kind), Some(count));
            assert!(!is_unsigned(kind));
        }
        for (kind, count) in [
            (gl::UNSIGNED_INT, 1),
            (UNSIGNED_INT_VEC2, 2),
            (UNSIGNED_INT_VEC3, 3),
            (UNSIGNED_INT_VEC4, 4),
        ] {
            assert!(is_unsigned(kind));
            assert_eq!(components(kind), Some(count));
        }
        assert_eq!(components(u32::MAX), None);
        assert_eq!(components(gl::FLOAT), None);
    }
}
