//! Typed vertex input owns format, range metadata and constant-value classification.
use super::{ApiVersion, Command, Kind, Result, WebGl, gl};
use serde_json::Value;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ValueKind {
    Float,
    Signed,
    Unsigned,
}

#[derive(Clone)]
pub(super) struct Attribute {
    pub(super) buffer: u32,
    pub(super) size: u32,
    pub(super) width: u32,
    pub(super) integer: bool,
    pub(super) stride: u32,
    pub(super) offset: u32,
    pub(super) enabled: bool,
    pub(super) divisor: u32,
    pub(super) kind: u32,
    pub(super) normalized: bool,
}
impl Default for Attribute {
    fn default() -> Self {
        Self {
            buffer: 0,
            size: 16,
            width: 4,
            integer: false,
            stride: 0,
            offset: 0,
            enabled: false,
            divisor: 0,
            kind: gl::FLOAT,
            normalized: false,
        }
    }
}

pub(super) fn component_bytes(kind: u32, api: ApiVersion) -> Result<u32> {
    match kind {
        gl::BYTE | gl::UNSIGNED_BYTE => Ok(1),
        gl::SHORT | gl::UNSIGNED_SHORT => Ok(2),
        gl::FLOAT => Ok(4),
        0x140b if api == ApiVersion::Two => Ok(2), // HALF_FLOAT, not HALF_FLOAT_OES
        gl::INT | gl::UNSIGNED_INT | 0x8368 | 0x8d9f if api == ApiVersion::Two => Ok(4),
        _ => Err(gl::INVALID_ENUM),
    }
}

impl WebGl {
    pub(super) fn vertex_pointer(&mut self, command: &Command) -> Result<Value> {
        let integer = command.op == "vertexAttribIPointer";
        if integer && self.options.api != ApiVersion::Two {
            return Err(gl::INVALID_OPERATION);
        }
        let index = command.u(0)? as usize;
        let width = command.u(1)?;
        let kind = command.u(2)?;
        let normalized = !integer && command.u(3)? != 0;
        let stride = command.u(if integer { 3 } else { 4 })?;
        let offset = command.u(if integer { 4 } else { 5 })?;
        if integer
            && ![
                gl::BYTE,
                gl::UNSIGNED_BYTE,
                gl::SHORT,
                gl::UNSIGNED_SHORT,
                gl::INT,
                gl::UNSIGNED_INT,
            ]
            .contains(&kind)
        {
            return Err(gl::INVALID_ENUM);
        }
        let component = component_bytes(kind, self.options.api)?;
        if !(1..=4).contains(&width) || stride > 255 || index >= self.attributes.len() {
            return Err(gl::INVALID_VALUE);
        }
        let packed = [0x8368, 0x8d9f].contains(&kind);
        if (packed && width != 4)
            || !offset.is_multiple_of(component)
            || !stride.is_multiple_of(component)
            || (self.array_buffer == 0 && offset != 0)
        {
            return Err(gl::INVALID_OPERATION);
        }
        if self.array_buffer != 0 {
            self.objects.get(self.array_buffer, Kind::Buffer)?;
            // SAFETY: only initialized buffer-backed offsets; never a client pointer.
            // The draw boundary independently checks all consumed byte ranges.
            unsafe {
                if integer {
                    let entry = self
                        .core
                        .as_ref()
                        .ok_or(gl::INVALID_OPERATION)?
                        .integer_pointer;
                    entry(
                        index as u32,
                        width as i32,
                        kind,
                        stride as i32,
                        offset as usize as *const _,
                    );
                } else {
                    gl::VertexAttribPointer(
                        index as u32,
                        width as i32,
                        kind,
                        u8::from(normalized),
                        stride as i32,
                        offset as usize as *const _,
                    );
                }
            }
            self.driver_result()?;
        }
        // Null-at-zero updates logical state without ever installing client arrays.
        let enabled = self.attributes[index].enabled;
        let divisor = self.attributes[index].divisor;
        self.objects
            .switch_buffer(self.attributes[index].buffer, self.array_buffer)?;
        self.attributes[index] = Attribute {
            buffer: self.array_buffer,
            size: if packed { 4 } else { width * component },
            width,
            integer,
            stride,
            offset,
            enabled,
            divisor,
            kind,
            normalized,
        };
        Ok(Value::Null)
    }

    pub(super) fn integer_attribute(&mut self, command: &Command) -> Result<Value> {
        if self.options.api != ApiVersion::Two {
            return Err(gl::INVALID_OPERATION);
        }
        let index = command.u(0)? as usize;
        if index >= self.attributes.len() {
            return Err(gl::INVALID_VALUE);
        }
        let core = self.core.as_ref().ok_or(gl::INVALID_OPERATION)?;
        let unsigned = matches!(
            command.op.as_str(),
            "vertexAttribI4ui" | "vertexAttribI4uiv"
        );
        let kind = if unsigned {
            let values = [command.u(1)?, command.u(2)?, command.u(3)?, command.u(4)?];
            // SAFETY: fixed four-component initialized vector and valid attribute slot.
            unsafe {
                (core.unsigned_attribute)(index as u32, values.as_ptr());
            }
            ValueKind::Unsigned
        } else {
            let values = [command.n(1)?, command.n(2)?, command.n(3)?, command.n(4)?];
            unsafe {
                (core.integer_attribute)(index as u32, values.as_ptr());
            }
            ValueKind::Signed
        };
        self.driver_result()?;
        self.attribute_values[index] = kind;
        Ok(Value::Null)
    }
}
