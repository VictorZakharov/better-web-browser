//! Pointer/state reflection uses browser ownership; constants use fixed native output sizes.
use super::{ApiVersion, Command, Result, WebGl, gl, json, vertex_attributes::ValueKind};
use serde_json::Value;

impl WebGl {
    pub(super) fn vertex_attribute_query(&mut self, command: &Command) -> Result<Value> {
        let index = command.u(0)? as usize;
        let attribute = self.attributes.get(index).ok_or(gl::INVALID_VALUE)?;
        let pname = command.u(1)?;
        if command.op == "getVertexAttribOffset" {
            if pname != gl::VERTEX_ATTRIB_ARRAY_POINTER {
                return Err(gl::INVALID_ENUM);
            }
            return Ok(json!(attribute.offset));
        }
        if pname == 0x88fe {
            if !self.extensions.instancing {
                return Err(gl::INVALID_ENUM);
            }
            return Ok(json!(attribute.divisor));
        }
        if pname == 0x88fd {
            if self.options.api != ApiVersion::Two {
                return Err(gl::INVALID_ENUM);
            }
            return Ok(json!(attribute.integer));
        }
        match pname {
            gl::VERTEX_ATTRIB_ARRAY_BUFFER_BINDING => return Ok(json!(attribute.buffer)),
            gl::VERTEX_ATTRIB_ARRAY_ENABLED => return Ok(json!(attribute.enabled)),
            gl::VERTEX_ATTRIB_ARRAY_SIZE => return Ok(json!(attribute.width)),
            gl::VERTEX_ATTRIB_ARRAY_STRIDE => return Ok(json!(attribute.stride)),
            gl::VERTEX_ATTRIB_ARRAY_TYPE => return Ok(json!(attribute.kind)),
            gl::VERTEX_ATTRIB_ARRAY_NORMALIZED => return Ok(json!(attribute.normalized)),
            gl::CURRENT_VERTEX_ATTRIB => {}
            _ => return Err(gl::INVALID_ENUM),
        }
        let (kind, values) = match self.attribute_values[index] {
            ValueKind::Float => {
                let mut values = [0f32; 4];
                // SAFETY: CURRENT_VERTEX_ATTRIB writes exactly four components.
                unsafe {
                    gl::GetVertexAttribfv(index as u32, pname, values.as_mut_ptr());
                }
                ("float", json!(values.map(super::float_values::encode)))
            }
            ValueKind::Signed => {
                let mut values = [0i32; 4];
                let entry = self
                    .core
                    .as_ref()
                    .ok_or(gl::INVALID_OPERATION)?
                    .get_integer_attribute;
                unsafe {
                    entry(index as u32, pname, values.as_mut_ptr());
                }
                ("int", json!(values))
            }
            ValueKind::Unsigned => {
                let mut values = [0u32; 4];
                let entry = self
                    .core
                    .as_ref()
                    .ok_or(gl::INVALID_OPERATION)?
                    .get_unsigned_attribute;
                unsafe {
                    entry(index as u32, pname, values.as_mut_ptr());
                }
                ("uint", json!(values))
            }
        };
        self.driver_result()?;
        Ok(if self.options.api == ApiVersion::One {
            values
        } else {
            json!({"kind":kind,"values":values})
        })
    }
}
