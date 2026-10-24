//! WebGL's extra feedback alias rules forbid undefined native read/write uses.
use super::{ApiVersion, Kind, Result, WebGl, gl};
const TARGET: u32 = super::core_buffers::TRANSFORM_FEEDBACK;
impl WebGl {
    pub(super) fn validate_transform_point_capacity(
        &self,
        count: u32,
        instances: u32,
    ) -> Result<()> {
        if self.options.api != ApiVersion::Two {
            return Ok(());
        }
        let record = &self.transform_feedback.records[&self.transform_feedback.bound];
        if !record.active || record.paused {
            return Ok(());
        }
        let mut varyings = 0;
        let native = self.objects.get(record.program, Kind::Program)?.native;
        unsafe { gl::GetProgramiv(native, 0x8c83, &mut varyings) };
        if varyings <= 0 {
            return Ok(());
        }
        let binding = record.bindings.first().ok_or(gl::INVALID_OPERATION)?;
        let object = self.objects.get(binding.id, Kind::Buffer)?;
        let available = object.bytes.len().saturating_sub(binding.offset);
        let available = binding.size.map_or(available, |size| available.min(size));
        // Every captured point writes at least one 32-bit component to binding
        // zero, in either separate or interleaved mode. This conservative lower
        // bound proves an oversized instanced draw cannot fit before the work
        // budget rejects it; never multiply vertex and instance counts in u32.
        let minimum = u64::from(count)
            .checked_mul(u64::from(instances))
            .and_then(|vertices| vertices.checked_mul(4))
            .ok_or(gl::INVALID_OPERATION)?;
        if minimum > available as u64 {
            return Err(gl::INVALID_OPERATION);
        }
        Ok(())
    }

    pub(super) fn validate_transform_buffer_use(&self, target: u32, id: u32) -> Result<()> {
        if self.options.api == ApiVersion::One || id == 0 {
            return Ok(());
        }
        let record = &self.transform_feedback.records[&self.transform_feedback.bound];
        if target != TARGET && record.bindings.iter().any(|binding| binding.id == id) {
            return Err(gl::INVALID_OPERATION);
        }
        Ok(())
    }
    pub(super) fn validate_transform_program_link(&self, id: u32) -> Result<()> {
        if self
            .transform_feedback
            .records
            .values()
            .any(|record| record.active && record.program == id)
        {
            return Err(gl::INVALID_OPERATION);
        }
        Ok(())
    }
    pub(super) fn validate_transform_draw(&self) -> Result<()> {
        if self.options.api == ApiVersion::One {
            return Ok(());
        }
        let record = &self.transform_feedback.records[&self.transform_feedback.bound];
        for attribute in self.attributes.iter().filter(|attribute| attribute.enabled) {
            self.validate_transform_buffer_use(gl::ARRAY_BUFFER, attribute.buffer)?;
        }
        // An active capture writes its indexed storage. Any other binding of the
        // same buffer (including a generic target) makes that use undefined.
        if record.active && !record.paused {
            for binding in record.bindings.iter().filter(|binding| binding.id != 0) {
                if self.array_buffer == binding.id
                    || self
                        .core_buffer_bindings
                        .iter()
                        .any(|(&target, &id)| target != TARGET && id == binding.id)
                    || self
                        .indexed_uniforms
                        .0
                        .iter()
                        .any(|uniform| uniform.id == binding.id)
                {
                    return Err(gl::INVALID_OPERATION);
                }
            }
        }
        // Only program-active uniform blocks are actually used by the draw.
        let native = self.objects.get(self.program, Kind::Program)?.native;
        let mut count = 0;
        unsafe { gl::GetProgramiv(native, 0x8a36, &mut count) };
        if !(0..=4096).contains(&count) {
            return Err(gl::INVALID_OPERATION);
        }
        let query = self.core.as_ref().unwrap().uniform_block_query;
        for index in 0..count as u32 {
            let mut binding = 0;
            unsafe { query(native, index, 0x8a3f, &mut binding) };
            if let Some(binding) = usize::try_from(binding)
                .ok()
                .and_then(|index| self.indexed_uniforms.0.get(index))
            {
                self.validate_transform_buffer_use(super::core_buffers::UNIFORM, binding.id)?;
            }
        }
        Ok(())
    }
}
