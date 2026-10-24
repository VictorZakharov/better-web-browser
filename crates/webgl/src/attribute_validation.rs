//! Reflect immutable program inputs once, but check mutable storage for every draw.
use super::{Kind, MAX_SHADER_BYTES, Result, WebGl, attribute_reflection, gl};

impl WebGl {
    fn active_attribute_mask(&mut self) -> Result<u32> {
        let object = self.objects.get(self.program, Kind::Program)?;
        let (program, generation) = (object.native, object.generation);
        if let Some(mask) = self.attribute_reflection.lookup(self.program, generation) {
            return Ok(mask);
        }
        #[cfg(test)]
        {
            self.attribute_reflection.native_scans += 1;
        }
        let mut count = 0;
        unsafe { gl::GetProgramiv(program, gl::ACTIVE_ATTRIBUTES, &mut count) };
        self.driver_result()?;
        if count < 0 || count as usize > self.attributes.len() {
            return Err(gl::INVALID_OPERATION);
        }
        let mut active = 0;
        // One bounded name buffer for the whole scan, rather than one per input
        // for every draw. Native reflection, not author declarations, is the key.
        let mut name = vec![0u8; MAX_SHADER_BYTES + 1];
        for index in 0..count as u32 {
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
            self.driver_result()?;
            if length < 0 || length as usize >= name.len() {
                return Err(gl::INVALID_OPERATION);
            }
            name[length as usize] = 0;
            let location = unsafe { gl::GetAttribLocation(program, name.as_ptr().cast()) };
            self.driver_result()?;
            active |= attribute_reflection::slots(location, kind, size, self.attributes.len())
                .ok_or(gl::INVALID_OPERATION)?;
        }
        self.attribute_reflection
            .insert(self.program, generation, active);
        Ok(active)
    }

    pub(super) fn validate_attributes(&mut self, maximum: u32) -> Result<()> {
        self.validate_instance_attributes(maximum, 1, false)
    }

    pub(super) fn validate_instance_attributes(
        &mut self,
        maximum: u32,
        instances: u32,
        instanced: bool,
    ) -> Result<()> {
        let active = self.active_attribute_mask()?;
        let mut per_vertex = false;
        // Even an inactive enabled array must have a live buffer. Only its byte
        // range is irrelevant to the linked shader; cache no mutable VAO state.
        if self
            .attributes
            .iter()
            .any(|attribute| attribute.enabled && attribute.buffer == 0)
        {
            return Err(gl::INVALID_OPERATION);
        }
        for (_, attribute) in self
            .attributes
            .iter()
            .enumerate()
            .filter(|(index, attribute)| attribute.enabled && active & (1u32 << index) != 0)
        {
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
}

#[cfg(test)]
mod tests;
