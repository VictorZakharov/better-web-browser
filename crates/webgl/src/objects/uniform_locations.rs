//! Uniform locations are small link-generation records, not GLES objects.
use super::{Result, gl, next_browser_name};
use std::collections::HashMap;

// Bound author-controlled metadata independently of the GPU object allowance.
// Two fixed-size maps replace a full Object (including image/buffer containers)
// for each location. This does not increase texture, buffer, or shader budgets.
pub(super) const MAX_LOCATIONS: usize = 16_384;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct UniformLocation {
    pub native: u32,
    pub owner: u32,
    pub generation: u32,
    pub uniform_type: u32,
}

#[derive(Default)]
pub(super) struct UniformLocations {
    records: HashMap<u32, UniformLocation>,
    names: HashMap<(u32, u32, u32), u32>,
}

impl UniformLocations {
    pub fn insert(&mut self, record: UniformLocation) -> Result<u32> {
        let key = (record.owner, record.generation, record.native);
        if let Some(id) = self.names.get(&key) {
            return Ok(*id);
        }
        if self.records.len() >= MAX_LOCATIONS {
            return Err(gl::OUT_OF_MEMORY);
        }
        let id = next_browser_name()?;
        self.records.insert(id, record);
        self.names.insert(key, id);
        Ok(id)
    }

    pub fn find(&self, owner: u32, generation: u32, native: u32) -> Option<u32> {
        self.names.get(&(owner, generation, native)).copied()
    }

    pub fn get(&self, id: u32) -> Result<&UniformLocation> {
        self.records.get(&id).ok_or(gl::INVALID_OPERATION)
    }

    pub fn retire(&mut self, owner: u32) {
        self.records.retain(|_, record| record.owner != owner);
        self.names.retain(|(program, _, _), _| *program != owner);
    }

    pub fn clear(&mut self) {
        self.records.clear();
        self.names.clear();
    }
}

#[cfg(test)]
mod tests;
