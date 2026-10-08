//! Buffer-only attributes with checked byte ranges before any driver draw call.
use super::{Command, Kind, MAX_DRAW_VERTICES, Result, WebGl, gl};
use serde_json::Value;

pub(super) use super::vertex_attributes::Attribute;
impl WebGl {
    pub(super) fn buffer_command(&mut self, c: &Command, bytes: Option<&[u8]>) -> Result<Value> {
        match c.op.as_str() {
            "bindBuffer" => {
                let target = c.u(0)?;
                let id = c.u(1)?;
                let native = self.objects.name(id, Kind::Buffer)?;
                if id != 0 && self.objects.get(id, Kind::Buffer)?.pending_delete {
                    return Err(gl::INVALID_OPERATION);
                }
                let extra = self.options.api == super::ApiVersion::Two
                    && super::core_buffers::extra_target(target);
                if ![gl::ARRAY_BUFFER, gl::ELEMENT_ARRAY_BUFFER].contains(&target) && !extra {
                    return Err(gl::INVALID_ENUM);
                }
                let class = if id == 0 {
                    0
                } else {
                    let previous = self.objects.get(id, Kind::Buffer)?.buffer_target;
                    let requested = super::core_buffers::classification(target);
                    if previous != 0 && requested != 0 && previous != requested {
                        return Err(gl::INVALID_OPERATION);
                    }
                    // Copy targets preserve an established class, but a first
                    // copy-target binding establishes the "other data" class.
                    // WebGL2 §5.1 Buffer Object Binding.
                    if requested == 0 {
                        if previous == 0 {
                            gl::ARRAY_BUFFER
                        } else {
                            previous
                        }
                    } else {
                        requested
                    }
                };
                unsafe {
                    gl::BindBuffer(target, native);
                }
                self.driver_result()?;
                if id != 0 {
                    self.objects.get_mut(id, Kind::Buffer)?.buffer_target = class;
                }
                match target {
                    gl::ARRAY_BUFFER => self.array_buffer = id,
                    gl::ELEMENT_ARRAY_BUFFER => {
                        self.objects.switch_buffer(self.element_buffer, id)?;
                        self.element_buffer = id;
                    }
                    _ if extra => {
                        self.core_buffer_bindings.insert(target, id);
                    }
                    _ => return Err(gl::INVALID_ENUM),
                }
            }
            "bufferData" => {
                return self.buffer_data(c, bytes.map(std::borrow::Cow::Borrowed));
            }
            "bufferSubData" => {
                let target = c.u(0)?;
                let id = self.bound_buffer(target)?;
                self.validate_transform_buffer_use(target, id)?;
                let offset = c.u(1)? as usize;
                let data = bytes.ok_or(gl::INVALID_VALUE)?;
                let end = offset.checked_add(data.len()).ok_or(gl::INVALID_VALUE)?;
                if end > self.objects.get(id, Kind::Buffer)?.bytes.len() {
                    return Err(gl::INVALID_VALUE);
                }
                unsafe {
                    gl::BufferSubData(
                        target,
                        offset as isize,
                        data.len() as isize,
                        data.as_ptr().cast(),
                    );
                }
                self.driver_result()?;
                self.index_cache.remove(id);
                let object = self.objects.get_mut(id, Kind::Buffer)?;
                object.bytes[offset..end].copy_from_slice(data);
                if offset == 0 && end == object.bytes.len() {
                    object.buffer_mirror_valid = true;
                }
            }
            "vertexAttribPointer" => return self.vertex_pointer(c),
            "enableVertexAttribArray" | "disableVertexAttribArray" => {
                let index = c.u(0)? as usize;
                let attribute = self.attributes.get_mut(index).ok_or(gl::INVALID_VALUE)?;
                attribute.enabled = c.op == "enableVertexAttribArray";
                unsafe {
                    if attribute.enabled {
                        gl::EnableVertexAttribArray(index as u32);
                    } else {
                        gl::DisableVertexAttribArray(index as u32);
                    }
                }
            }
            "drawArrays" => {
                let mode = checked_mode(c.u(0)?)?;
                let first = c.u(1)?;
                let count = c.u(2)?;
                if count > MAX_DRAW_VERTICES
                    || first
                        .checked_add(count.saturating_sub(1))
                        .is_none_or(|end| end > i32::MAX as u32)
                {
                    return Err(gl::INVALID_VALUE);
                }
                if count != 0 {
                    self.validate_attributes(
                        first.checked_add(count - 1).ok_or(gl::INVALID_OPERATION)?,
                    )?;
                }
                self.validate_program()?;
                let captured =
                    self.prepare_transform_capture(mode, false, std::iter::once((count, 1)))?;
                let _sampling = self.sampling_guard()?;
                self.validate_framebuffer()?;
                self.native_draw_arrays(mode, first as i32, count as i32, None);
                self.driver_result()?;
                self.record_transform_capture(captured);
            }
            "drawElements" => {
                let mode = checked_mode(c.u(0)?)?;
                let count = c.u(1)? as usize;
                let kind = c.u(2)?;
                let offset = c.u(3)? as usize;
                let size = self.index_size(kind)?;
                if count > MAX_DRAW_VERTICES as usize {
                    return Err(gl::INVALID_VALUE);
                }
                if !offset.is_multiple_of(size) {
                    return Err(gl::INVALID_OPERATION);
                }
                if count == 0 {
                    self.validate_program()?;
                    return Ok(Value::Null);
                }
                let maximum = self.maximum_index(count, size, offset)?;
                if let Some(maximum) = maximum {
                    self.validate_attributes(maximum)?;
                }
                self.validate_program()?;
                let _sampling = self.sampling_guard()?;
                self.validate_framebuffer()?;
                self.native_draw_elements(mode, count as i32, kind, offset, None);
            }
            _ => return Err(gl::INVALID_OPERATION),
        }
        self.driver_result()?;
        Ok(Value::Null)
    }
    pub(super) fn bound_buffer(&self, target: u32) -> Result<u32> {
        let id = match target {
            gl::ARRAY_BUFFER => self.array_buffer,
            gl::ELEMENT_ARRAY_BUFFER => self.element_buffer,
            _ if self.options.api == super::ApiVersion::Two
                && super::core_buffers::extra_target(target) =>
            {
                *self.core_buffer_bindings.get(&target).unwrap_or(&0)
            }
            _ => return Err(gl::INVALID_ENUM),
        };
        self.objects.get(id, Kind::Buffer)?;
        Ok(id)
    }
    pub(super) fn validate_program(&self) -> Result<()> {
        self.validate_transform_draw()?;
        let program = self.objects.get(self.program, Kind::Program)?;
        let mut linked = 0;
        unsafe {
            gl::GetProgramiv(program.native, gl::LINK_STATUS, &mut linked);
        }
        if linked == 0 {
            Err(gl::INVALID_OPERATION)
        } else {
            Ok(())
        }
    }
}
pub(super) fn checked_mode(mode: u32) -> Result<u32> {
    if [
        gl::POINTS,
        gl::LINES,
        gl::LINE_LOOP,
        gl::LINE_STRIP,
        gl::TRIANGLES,
        gl::TRIANGLE_STRIP,
        gl::TRIANGLE_FAN,
    ]
    .contains(&mode)
    {
        Ok(mode)
    } else {
        Err(gl::INVALID_ENUM)
    }
}
