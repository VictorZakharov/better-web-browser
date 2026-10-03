//! Indexed uniform ranges retain their buffers and track base-vs-range semantics.
use super::{ApiVersion, Command, Kind, Result, WebGl, gl};
use serde_json::{Value, json};
const UNIFORM: u32 = super::core_buffers::UNIFORM;

#[derive(Clone, Copy, Default)]
pub(super) struct Binding {
    pub id: u32,
    pub offset: usize,
    pub size: Option<usize>,
}
pub(super) struct Bindings(pub Vec<Binding>);
impl Bindings {
    pub(super) fn new(api: ApiVersion) -> std::result::Result<Self, String> {
        if api == ApiVersion::One {
            return Ok(Self(Vec::new()));
        }
        let mut count = 0;
        unsafe { gl::GetIntegerv(0x8a2f, &mut count) };
        if !(1..=128).contains(&count) {
            return Err("ANGLE indexed uniform limit is outside the admitted bound".into());
        }
        Ok(Self(vec![Binding::default(); count as usize]))
    }
}

impl WebGl {
    pub(super) fn indexed_uniform_command(&mut self, c: &Command) -> Result<Value> {
        if self.options.api != ApiVersion::Two {
            return Err(gl::INVALID_OPERATION);
        }
        let target = c.u(0)?;
        let index = c.u(1)? as usize;
        if c.op == "getIndexedParameter" {
            if ![0x8a28, 0x8a29, 0x8a2a].contains(&target) {
                return Err(gl::INVALID_ENUM);
            }
            let binding = *self
                .indexed_uniforms
                .0
                .get(index)
                .ok_or(gl::INVALID_VALUE)?;
            return Ok(if target == 0x8a28 {
                if binding.id == 0 {
                    Value::Null
                } else {
                    json!(binding.id)
                }
            } else if target == 0x8a29 {
                json!(binding.offset)
            } else if binding.id == 0 {
                json!(0)
            } else {
                json!(
                    binding
                        .size
                        .unwrap_or(self.objects.get(binding.id, Kind::Buffer)?.bytes.len())
                )
            });
        }
        if target != UNIFORM {
            return Err(gl::INVALID_ENUM);
        }
        let previous = *self
            .indexed_uniforms
            .0
            .get(index)
            .ok_or(gl::INVALID_VALUE)?;
        let id = c.u(2)?;
        let native = self.objects.name(id, Kind::Buffer)?;
        let mut binding = Binding {
            id,
            offset: 0,
            size: None,
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
                    || offset
                        .checked_add(size)
                        .is_none_or(|end| end > object.bytes.len())
                {
                    return Err(gl::INVALID_VALUE);
                }
                let mut alignment = 0;
                unsafe { gl::GetIntegerv(0x8a34, &mut alignment) };
                if alignment < 1 || !offset.is_multiple_of(alignment as usize) {
                    return Err(gl::INVALID_VALUE);
                }
                binding.offset = offset;
                binding.size = Some(size);
            }
        }
        let core = self.core.as_ref().ok_or(gl::INVALID_OPERATION)?;
        unsafe {
            if c.op == "bindBufferBase" {
                (core.indexed_buffer_base)(target, index as u32, native);
            } else {
                (core.indexed_buffer_range)(
                    target,
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
        self.indexed_uniforms.0[index] = binding;
        // Both indexed binding APIs also change the generic target binding.
        self.core_buffer_bindings.insert(target, id);
        Ok(Value::Null)
    }

    pub(super) fn detach_indexed_uniform(&mut self, id: u32) -> Result<()> {
        if self.options.api == ApiVersion::One {
            return Ok(());
        }
        let function = self
            .core
            .as_ref()
            .ok_or(gl::INVALID_OPERATION)?
            .indexed_buffer_base;
        for (index, binding) in self.indexed_uniforms.0.iter_mut().enumerate() {
            if binding.id == id {
                unsafe { function(UNIFORM, index as u32, 0) };
                self.objects.switch_buffer(id, 0)?;
                *binding = Binding::default();
            }
        }
        // BindBufferBase also alters the generic binding, unlike deletion of an
        // indexed reference. Restore a different live generic buffer, if any.
        let generic = *self.core_buffer_bindings.get(&UNIFORM).unwrap_or(&0);
        let native = self.objects.name(generic, Kind::Buffer)?;
        unsafe { gl::BindBuffer(UNIFORM, native) };
        self.driver_result()?;
        Ok(())
    }
}
