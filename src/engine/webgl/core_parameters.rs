//! GLES3 capabilities use fixed scalar types; pointer-valued state stays private.
use super::{ApiVersion, Result, WebGl, gl, json};
use serde_json::Value;

impl WebGl {
    pub(super) fn core_parameter(&mut self, pname: u32) -> Result<Option<Value>> {
        if [0x8e25, 0x8e24, 0x8e23].contains(&pname) {
            if self.options.api != ApiVersion::Two {
                return Err(gl::INVALID_ENUM);
            }
            let id = self.transform_feedback.bound;
            let record = &self.transform_feedback.records[&id];
            return Ok(Some(match pname {
                0x8e25 => {
                    if id == 0 {
                        Value::Null
                    } else {
                        json!(id)
                    }
                }
                0x8e24 => json!(record.active),
                _ => json!(record.paused),
            }));
        }
        if pname == 0x9247 {
            if self.options.api != ApiVersion::Two {
                return Err(gl::INVALID_ENUM);
            }
            // WebGL explicitly permits zero to protect the main-thread budget.
            return Ok(Some(json!(0)));
        }
        if pname == 0x8919 {
            if self.options.api != ApiVersion::Two {
                return Err(gl::INVALID_ENUM);
            }
            let id = self.samplers[self.texture_unit];
            return Ok(Some(if id == 0 { Value::Null } else { json!(id) }));
        }
        let integer64 = [0x8a30, 0x8a31, 0x8a33, 0x8d6b, 0x9111].contains(&pname);
        let integer = [
            0x0d02, 0x0d03, 0x0d04, 0x0cf2, 0x0cf3, 0x0cf4, 0x806d, 0x806e, 0x8073,
            0x88ff, // 3D texture size, array texture layers
            0x80e8, 0x80e9, // recommended element/vertex counts
            0x8b4a, 0x8b49, 0x8b4b, // vertex/fragment/varying components
            0x8a2b, 0x8a2d, 0x8a2e, 0x8a2f, 0x8a34, // uniform blocks/bindings/alignment
            0x8904, 0x8905, // legal program texel offsets, signed
            0x8c80, 0x8c8a, 0x8c8b, // transform feedback limits
            0x8d57, // samples
            0x9122, 0x9125, // vertex output / fragment input components
        ]
        .contains(&pname);
        if !integer64 && !integer && pname != 0x8c89 {
            return Ok(None);
        }
        if self.options.api != ApiVersion::Two {
            return Err(gl::INVALID_ENUM);
        }
        let core = self.core.as_ref().ok_or(gl::INVALID_OPERATION)?;
        if pname == 0x8d6b {
            return Ok(Some(json!(core.max_element_index)));
        }
        if pname == 0x8c89 {
            return Ok(Some(json!(unsafe { gl::IsEnabled(pname) != 0 })));
        }
        if integer64 {
            let mut value = 0i64;
            unsafe {
                (core.get_integer64)(pname, &mut value);
            }
            self.driver_result()?;
            return Ok(Some(json!(value)));
        }
        let mut value = 0i32;
        // SAFETY: only the fixed scalar capability enum list above.
        unsafe {
            gl::GetIntegerv(pname, &mut value);
        }
        self.driver_result()?;
        Ok(Some(json!(match pname {
            0x8073 => value.min(4096),
            // Allocation budgets constrain actual storage separately from this dimension cap.
            0x88ff => value.min(256),
            _ => value,
        })))
    }
}
