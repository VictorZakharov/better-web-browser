//! GLES3 buffer transfers use bounded offsets and owned native readback bytes.
//! https://registry.khronos.org/webgl/specs/latest/2.0/#3.7.3
use super::{ApiVersion, Command, Kind, MAX_UPLOAD_BYTES, Result, WebGl, gl, json};
use serde_json::Value;

pub(super) const COPY_READ: u32 = 0x8f36;
pub(super) const COPY_WRITE: u32 = 0x8f37;
pub(super) const UNIFORM: u32 = 0x8a11;
pub(super) const PIXEL_PACK: u32 = 0x88eb;
pub(super) const PIXEL_UNPACK: u32 = 0x88ec;
pub(super) const TRANSFORM_FEEDBACK: u32 = 0x8c8e;

pub(super) fn extra_target(target: u32) -> bool {
    [
        COPY_READ,
        COPY_WRITE,
        UNIFORM,
        PIXEL_PACK,
        PIXEL_UNPACK,
        TRANSFORM_FEEDBACK,
    ]
    .contains(&target)
}

pub(super) fn classification(target: u32) -> u32 {
    match target {
        COPY_READ | COPY_WRITE => 0,
        gl::ELEMENT_ARRAY_BUFFER => gl::ELEMENT_ARRAY_BUFFER,
        _ => gl::ARRAY_BUFFER,
    }
}

impl WebGl {
    pub(super) fn core_buffer_parameter(&self, pname: u32) -> Result<Option<Value>> {
        let target = match pname {
            COPY_READ | COPY_WRITE => pname,
            0x8a28 => UNIFORM,
            0x88ed => PIXEL_PACK,
            0x88ef => PIXEL_UNPACK,
            0x8c8f => TRANSFORM_FEEDBACK,
            _ => return Ok(None),
        };
        if self.options.api != ApiVersion::Two {
            return Err(gl::INVALID_ENUM);
        }
        let id = *self.core_buffer_bindings.get(&target).unwrap_or(&0);
        Ok(Some(if id == 0 { Value::Null } else { json!(id) }))
    }

    pub(super) fn core_buffer_command(&mut self, command: &Command) -> Result<Value> {
        if self.options.api != ApiVersion::Two {
            return Err(gl::INVALID_OPERATION);
        }
        match command.op.as_str() {
            "copyBufferSubData" => {
                let read = command.u(0)?;
                let write = command.u(1)?;
                let source = self.bound_buffer(read)?;
                let destination = self.bound_buffer(write)?;
                self.validate_transform_buffer_use(read, source)?;
                self.validate_transform_buffer_use(write, destination)?;
                let source_offset = nonnegative(command, 2)?;
                let destination_offset = nonnegative(command, 3)?;
                let size = nonnegative(command, 4)?;
                let source_end = range_end(
                    source_offset,
                    size,
                    self.objects.get(source, Kind::Buffer)?.bytes.len(),
                )?;
                let destination_end = range_end(
                    destination_offset,
                    size,
                    self.objects.get(destination, Kind::Buffer)?.bytes.len(),
                )?;
                if source == destination
                    && source_offset < destination_end
                    && destination_offset < source_end
                {
                    return Err(gl::INVALID_VALUE);
                }
                // WebGL2 disallows copying across element/non-element classes,
                // even when both buffers are bound through the neutral copy targets.
                let source_class = self.objects.get(source, Kind::Buffer)?.buffer_target;
                let destination_class = self.objects.get(destination, Kind::Buffer)?.buffer_target;
                if (source_class == gl::ELEMENT_ARRAY_BUFFER)
                    != (destination_class == gl::ELEMENT_ARRAY_BUFFER)
                {
                    return Err(gl::INVALID_OPERATION);
                }
                let entries = self.core.as_ref().ok_or(gl::INVALID_OPERATION)?;
                // SAFETY: both native bindings are browser-owned, ranges are within
                // initialized storage, and all values fit the native pointer-size ABI.
                unsafe {
                    (entries.copy_buffer)(
                        read,
                        write,
                        source_offset as isize,
                        destination_offset as isize,
                        size as isize,
                    );
                }
                self.driver_result()?;
                // Keep indexed-draw validation synchronized with native copies.
                let copied = self.objects.get(source, Kind::Buffer)?.bytes
                    [source_offset..source_end]
                    .to_vec();
                self.objects.get_mut(destination, Kind::Buffer)?.bytes
                    [destination_offset..destination_end]
                    .copy_from_slice(&copied);
                Ok(Value::Null)
            }
            "getBufferSubData" => Ok(json!(self.read_buffer(command)?)),
            _ => Err(gl::INVALID_OPERATION),
        }
    }

    pub(super) fn read_buffer(&mut self, command: &Command) -> Result<Vec<u8>> {
        if self.options.api != ApiVersion::Two {
            return Err(gl::INVALID_OPERATION);
        }
        let target = command.u(0)?;
        let id = self.bound_buffer(target)?;
        if target == TRANSFORM_FEEDBACK && self.transform_feedback.any_active() {
            return Err(gl::INVALID_OPERATION);
        }
        self.validate_transform_buffer_use(target, id)?;
        let offset = nonnegative(command, 1)?;
        let size = nonnegative(command, 2)?;
        range_end(
            offset,
            size,
            self.objects.get(id, Kind::Buffer)?.bytes.len(),
        )?;
        if size > MAX_UPLOAD_BYTES {
            return Err(gl::OUT_OF_MEMORY);
        }
        if size == 0 {
            return Ok(Vec::new());
        }
        let entries = self.core.as_ref().ok_or(gl::INVALID_OPERATION)?;
        let (map, unmap) = (entries.map_buffer, entries.unmap_buffer);
        // GL_MAP_READ_BIT only. The pointer stays on the native owner thread;
        // neither the realm nor renderer IPC receives mapped native storage.
        let pointer = unsafe { map(target, offset as isize, size as isize, 1) };
        self.driver_result()?;
        if pointer.is_null() {
            return Err(gl::OUT_OF_MEMORY);
        }
        let bytes = unsafe { std::slice::from_raw_parts(pointer.cast::<u8>(), size) }.to_vec();
        // SAFETY: successful map on this target; copy completed before unmapping.
        let valid = unsafe { unmap(target) };
        self.driver_result()?;
        if valid == 0 {
            return Err(gl::INVALID_OPERATION);
        }
        Ok(bytes)
    }

    pub(super) fn detach_core_buffer(&mut self, id: u32) {
        for (target, binding) in &mut self.core_buffer_bindings {
            if *binding == id {
                *binding = 0;
                // SAFETY: fixed admitted buffer targets; no author pointer.
                unsafe {
                    gl::BindBuffer(*target, 0);
                }
            }
        }
    }
}

fn nonnegative(command: &Command, index: usize) -> Result<usize> {
    command
        .i
        .get(index)
        .copied()
        .and_then(|value| usize::try_from(value).ok())
        .filter(|value| *value <= isize::MAX as usize)
        .ok_or(gl::INVALID_VALUE)
}

fn range_end(offset: usize, size: usize, capacity: usize) -> Result<usize> {
    offset
        .checked_add(size)
        .filter(|end| *end <= capacity)
        .ok_or(gl::INVALID_VALUE)
}
