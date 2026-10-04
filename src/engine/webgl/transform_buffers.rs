//! Object-local capture buffer bindings retain browser storage until detached.
use super::indexed_uniform_buffers::Binding;
use super::{ApiVersion, Command, Kind, Result, WebGl, gl};
use serde_json::{Value, json};
const TARGET: u32 = super::core_buffers::TRANSFORM_FEEDBACK;
impl WebGl {
    pub(super) fn transform_buffer_command(&mut self, c: &Command) -> Result<Value> {
        if self.options.api != ApiVersion::Two {
            return Err(gl::INVALID_OPERATION);
        }
        let target = c.u(0)?;
        let index = c.u(1)? as usize;
        let bound = self.transform_feedback.bound;
        let record = &self.transform_feedback.records[&bound];
        let previous = *record.bindings.get(index).ok_or(gl::INVALID_VALUE)?;
        if c.op == "getIndexedParameter" {
            return Ok(match target {
                0x8c8f => {
                    if previous.id == 0 {
                        Value::Null
                    } else {
                        json!(previous.id)
                    }
                }
                0x8c84 => json!(previous.offset),
                0x8c85 => {
                    if previous.id == 0 {
                        json!(0)
                    } else {
                        json!(
                            previous.size.unwrap_or(
                                self.objects.get(previous.id, Kind::Buffer)?.bytes.len()
                            )
                        )
                    }
                }
                _ => return Err(gl::INVALID_ENUM),
            });
        }
        if target != TARGET {
            return Err(gl::INVALID_ENUM);
        }
        if record.active {
            return Err(gl::INVALID_OPERATION);
        }
        let id = c.u(2)?;
        let native = self.objects.name(id, Kind::Buffer)?;
        let mut binding = Binding {
            id,
            ..Binding::default()
        };
        if id != 0 {
            let object = self.objects.get(id, Kind::Buffer)?;
            if object.pending_delete || object.buffer_target == gl::ELEMENT_ARRAY_BUFFER {
                return Err(gl::INVALID_OPERATION);
            }
            if c.op == "bindBufferRange" {
                let offset =
                    c.i.get(3)
                        .and_then(|value| usize::try_from(*value).ok())
                        .ok_or(gl::INVALID_VALUE)?;
                let size =
                    c.i.get(4)
                        .and_then(|value| usize::try_from(*value).ok())
                        .ok_or(gl::INVALID_VALUE)?;
                if size == 0
                    || !offset.is_multiple_of(4)
                    || !size.is_multiple_of(4)
                    || offset
                        .checked_add(size)
                        .is_none_or(|end| end > object.bytes.len())
                {
                    return Err(gl::INVALID_VALUE);
                }
                binding.offset = offset;
                binding.size = Some(size);
            }
        }
        let core = self.core.as_ref().unwrap();
        unsafe {
            if c.op == "bindBufferBase" || id == 0 {
                (core.indexed_buffer_base)(TARGET, index as u32, native);
            } else {
                (core.indexed_buffer_range)(
                    TARGET,
                    index as u32,
                    native,
                    binding.offset as isize,
                    binding.size.unwrap_or(0) as isize,
                );
            }
        }
        self.driver_result()?;
        self.objects.switch_buffer(previous.id, id)?;
        if id != 0 {
            self.objects.get_mut(id, Kind::Buffer)?.buffer_target = gl::ARRAY_BUFFER;
        }
        self.transform_feedback
            .records
            .get_mut(&bound)
            .unwrap()
            .bindings[index] = binding;
        self.core_buffer_bindings.insert(TARGET, id);
        Ok(Value::Null)
    }
    pub(super) fn detach_transform_buffer(&mut self, id: u32) -> Result<()> {
        if self.options.api == ApiVersion::One {
            return Ok(());
        }
        let bound = self.transform_feedback.bound;
        let record = self.transform_feedback.records.get_mut(&bound).unwrap();
        let native_deleted = self.objects.get(id, Kind::Buffer)?.native_deleted;
        let function = self.core.as_ref().unwrap().indexed_buffer_base;
        for (index, binding) in record.bindings.iter_mut().enumerate() {
            if binding.id == id {
                // DeleteBuffers already detached the native current container.
                // Rebinding its active capture would itself be an invalid call.
                if !native_deleted {
                    if record.active {
                        return Err(gl::INVALID_OPERATION);
                    }
                    unsafe { function(TARGET, index as u32, 0) };
                }
                self.objects.release(id);
                *binding = Binding::default();
            }
        }
        let generic = *self.core_buffer_bindings.get(&TARGET).unwrap_or(&0);
        let generic = if generic == id { 0 } else { generic };
        unsafe { gl::BindBuffer(TARGET, self.objects.name(generic, Kind::Buffer)?) };
        self.driver_result()
    }
}
