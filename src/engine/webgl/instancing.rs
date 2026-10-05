//! Native instanced draws with independent vertex and instance byte-range checks.
use super::{Command, MAX_DRAW_VERTICES, Result, WebGl, buffers::checked_mode, gl};
use serde_json::Value;

impl WebGl {
    pub(super) fn instanced_command(&mut self, c: &Command) -> Result<Value> {
        if !self.extensions.instancing {
            return Err(gl::INVALID_OPERATION);
        }
        if ["vertexAttribDivisorANGLE", "vertexAttribDivisor"].contains(&c.op.as_str()) {
            let index = c.u(0)? as usize;
            let divisor = c.u(1)?;
            if index >= self.attributes.len() {
                return Err(gl::INVALID_VALUE);
            }
            let function = self.extensions.divisor.ok_or(gl::INVALID_OPERATION)?;
            unsafe {
                function(index as u32, divisor);
            }
            self.driver_result()?;
            self.attributes[index].divisor = divisor;
            return Ok(Value::Null);
        }
        let mode = checked_mode(c.u(0)?)?;
        let indexed =
            ["drawElementsInstancedANGLE", "drawElementsInstanced"].contains(&c.op.as_str());
        let count = c.u(if indexed { 1 } else { 2 })?;
        let instances = c.u(if indexed { 4 } else { 3 })?;
        // Protect the software rasterizer from multiplicative work even when each
        // individual buffer access fits. This is the same admitted draw budget as
        // non-instanced draws, counted across all instances rather than per copy.
        if count > MAX_DRAW_VERTICES
            || instances > MAX_DRAW_VERTICES
            || count
                .checked_mul(instances)
                .is_none_or(|work| work > MAX_DRAW_VERTICES)
        {
            if mode == gl::POINTS {
                self.validate_transform_point_capacity(count, instances)?;
            }
            return Err(gl::INVALID_VALUE);
        }
        self.validate_program()?;
        self.validate_framebuffer()?;
        if indexed {
            let kind = c.u(2)?;
            let offset = c.u(3)? as usize;
            let size = self.index_size(kind)?;
            if !offset.is_multiple_of(size) {
                return Err(gl::INVALID_OPERATION);
            }
            if count == 0 || instances == 0 {
                return Ok(Value::Null);
            }
            let maximum = self.maximum_index(count as usize, size, offset)?;
            if let Some(maximum) = maximum {
                self.validate_instance_attributes(maximum, instances, true)?;
            }
            let function = self.extensions.elements.ok_or(gl::INVALID_OPERATION)?;
            let _sampling = self.sampling_guard()?;
            unsafe {
                function(
                    mode,
                    count as i32,
                    kind,
                    offset as *const _,
                    instances as i32,
                );
            }
        } else {
            let first = c.u(1)?;
            let end = first
                .checked_add(count)
                .filter(|end| first <= i32::MAX as u32 && *end <= i32::MAX as u32 + 1)
                .ok_or(gl::INVALID_VALUE)?;
            if count == 0 || instances == 0 {
                return Ok(Value::Null);
            }
            self.validate_instance_attributes(end - 1, instances, true)?;
            let function = self.extensions.arrays.ok_or(gl::INVALID_OPERATION)?;
            let _sampling = self.sampling_guard()?;
            unsafe {
                function(mode, first as i32, count as i32, instances as i32);
            }
        }
        self.driver_result()?;
        Ok(Value::Null)
    }
}
