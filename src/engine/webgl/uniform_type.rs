//! Scan linked native reflection once per generation, with a bounded metadata cache.
use super::{Kind, MAX_SHADER_BYTES, Result, WebGl, gl, uniform_reflection};

impl WebGl {
    pub(super) fn uniform_type(&mut self, owner: u32, name: &str) -> Result<u32> {
        let object = self.objects.get(owner, Kind::Program)?;
        let (program, generation) = (object.native, object.generation);
        let wanted = super::uniform_queries::array_family(name);
        if let Some(kind) = self.uniform_reflection.lookup(owner, generation, &wanted) {
            return kind.ok_or(gl::INVALID_OPERATION);
        }
        #[cfg(test)]
        {
            self.uniform_reflection.native_scans += 1;
        }
        let mut count = 0;
        unsafe {
            gl::GetProgramiv(program, gl::ACTIVE_UNIFORMS, &mut count);
        }
        self.driver_result()?;
        if !(0..=16_384).contains(&count) {
            return Err(gl::OUT_OF_MEMORY);
        }
        let mut builder = uniform_reflection::Builder::new(count as usize);
        let mut found = None;
        let mut bytes = vec![0; MAX_SHADER_BYTES];
        for index in 0..count as u32 {
            let (mut written, mut size, mut kind) = (0, 0, 0);
            unsafe {
                gl::GetActiveUniform(
                    program,
                    index,
                    bytes.len() as i32,
                    &mut written,
                    &mut size,
                    &mut kind,
                    bytes.as_mut_ptr().cast(),
                );
            }
            self.driver_result()?;
            let end = (written.max(0) as usize).min(bytes.len());
            let reflected = String::from_utf8_lossy(&bytes[..end]);
            let family = super::uniform_queries::array_family(&reflected);
            // getUniformLocation already validated the actual native location.
            // Families preserve nested struct fields without assuming numeric
            // locations are consecutive. Keep the first native match.
            if found.is_none() && family == wanted {
                found = Some(kind);
            }
            if let Some(staging) = builder.as_mut()
                && !staging.push(family, kind)
            {
                // Cache pressure is not WebGL resource admission: continue
                // checking the real program and return its actual wanted type.
                builder = None;
            }
        }
        if let Some(staging) = builder {
            self.uniform_reflection
                .insert(owner, generation, staging.finish());
        }
        found.ok_or(gl::INVALID_OPERATION)
    }
}
