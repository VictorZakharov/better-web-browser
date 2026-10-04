//! Buffer-only attributes with checked byte ranges before any driver draw call.
use super::{Command, Kind, MAX_DRAW_VERTICES, MAX_UPLOAD_BYTES, Result, WebGl, gl};
use serde_json::Value;
use std::ptr;

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
                let target = c.u(0)?;
                let id = self.bound_buffer(target)?;
                self.validate_transform_buffer_use(target, id)?;
                let size = c.u(1)? as usize;
                let usage = c.u(2)?;
                let core_usage = self.options.api == super::ApiVersion::Two
                    && [0x88e1, 0x88e2, 0x88e5, 0x88e6, 0x88e9, 0x88ea].contains(&usage);
                if ![gl::STATIC_DRAW, gl::DYNAMIC_DRAW, gl::STREAM_DRAW].contains(&usage)
                    && !core_usage
                {
                    return Err(gl::INVALID_ENUM);
                }
                if size > MAX_UPLOAD_BYTES || bytes.is_some_and(|b| b.len() != size) {
                    return Err(gl::INVALID_VALUE);
                }
                let previous = self.objects.get(id, Kind::Buffer)?.capacity;
                self.charge(previous, size)?;
                // Use initialized storage even for numeric bufferData allocations.
                let data = bytes.map_or_else(|| vec![0; size], <[u8]>::to_vec);
                unsafe {
                    gl::BufferData(target, size as isize, data.as_ptr().cast(), usage);
                }
                self.driver_result()?;
                let object = self.objects.get_mut(id, Kind::Buffer)?;
                object.capacity = previous.max(size);
                object.bytes = data;
                object.buffer_mirror_valid = true;
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
                        .checked_add(count)
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
                let _sampling = self.sampling_guard()?;
                self.validate_framebuffer()?;
                unsafe {
                    gl::DrawArrays(mode, first as i32, count as i32);
                }
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
                unsafe {
                    gl::DrawElements(
                        mode,
                        count as i32,
                        kind,
                        if offset == 0 {
                            ptr::null()
                        } else {
                            offset as *const _
                        },
                    );
                }
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
    pub(super) fn validate_attributes(&self, maximum: u32) -> Result<()> {
        self.validate_instance_attributes(maximum, 1, false)
    }
    pub(super) fn validate_instance_attributes(
        &self,
        maximum: u32,
        instances: u32,
        instanced: bool,
    ) -> Result<()> {
        let program = self.objects.get(self.program, Kind::Program)?.native;
        let mut count = 0;
        unsafe {
            gl::GetProgramiv(program, gl::ACTIVE_ATTRIBUTES, &mut count);
        }
        let mut active = vec![false; self.attributes.len()];
        for index in 0..count.max(0) as u32 {
            let mut name = vec![0u8; super::MAX_SHADER_BYTES + 1];
            let (mut length, mut size, mut kind) = (0, 0, 0);
            unsafe {
                gl::GetActiveAttrib(
                    program,
                    index,
                    name.len() as i32,
                    &mut length,
                    &mut size,
                    &mut kind,
                    name.as_mut_ptr().cast(),
                );
            }
            if length < 0 || length as usize >= name.len() {
                return Err(gl::INVALID_OPERATION);
            }
            let location = unsafe { gl::GetAttribLocation(program, name.as_ptr().cast()) };
            let columns = match kind {
                gl::FLOAT_MAT2 => 2,
                gl::FLOAT_MAT3 => 3,
                gl::FLOAT_MAT4 => 4,
                0x8b65 | 0x8b66 => 2,
                0x8b67 | 0x8b68 => 3,
                0x8b69 | 0x8b6a => 4,
                _ => 1,
            };
            for offset in 0..columns * size.max(1) {
                if let Some(value) = usize::try_from(location + offset)
                    .ok()
                    .and_then(|index| active.get_mut(index))
                {
                    *value = true;
                }
            }
        }
        let mut per_vertex = false;
        // WebGL requires a buffer for *every* enabled array, even when the current
        // shader does not consume it. Active attributes alone govern byte ranges.
        if self
            .attributes
            .iter()
            .any(|attribute| attribute.enabled && attribute.buffer == 0)
        {
            return Err(gl::INVALID_OPERATION);
        }
        for (index, attribute) in self
            .attributes
            .iter()
            .enumerate()
            .filter(|(index, a)| a.enabled && active[*index])
        {
            let _ = index;
            per_vertex |= attribute.divisor == 0;
            let maximum = (instances - 1)
                .checked_div(attribute.divisor)
                .unwrap_or(maximum);
            let buffer = self.objects.get(attribute.buffer, Kind::Buffer)?;
            let stride = if attribute.stride == 0 {
                attribute.size
            } else {
                attribute.stride
            };
            let end = u64::from(attribute.offset)
                + u64::from(maximum) * u64::from(stride)
                + u64::from(attribute.size);
            if end > buffer.bytes.len() as u64 {
                return Err(gl::INVALID_OPERATION);
            }
        }
        if instanced && !per_vertex && self.options.api == super::ApiVersion::One {
            return Err(gl::INVALID_OPERATION);
        }
        Ok(())
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
