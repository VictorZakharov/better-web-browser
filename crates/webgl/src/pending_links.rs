//! Bounded owner-thread submissions, not synthetic native completion results.
use std::collections::VecDeque;

pub(super) const LIMIT: usize = 128;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Shader {
    pub id: u32,
    pub native: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Link {
    pub id: u32,
    pub native: u32,
    pub shaders: Vec<Shader>,
}

#[derive(Default)]
pub(super) struct PendingLinks {
    entries: VecDeque<Link>,
}

impl PendingLinks {
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn front(&self) -> Option<&Link> {
        self.entries.front()
    }

    pub fn pop(&mut self) -> Option<Link> {
        self.entries.pop_front()
    }

    pub fn push(&mut self, link: Link) -> Result<(), Link> {
        if self.entries.len() >= LIMIT || link.shaders.len() > 2 || self.contains(link.id) {
            return Err(link);
        }
        self.entries.push_back(link);
        Ok(())
    }

    pub fn contains(&self, id: u32) -> bool {
        self.entries.iter().any(|link| link.id == id)
    }

    /// A barrier submits the entire prefix, preserving API invocation order.
    pub fn through_program(&self, id: u32) -> usize {
        self.entries
            .iter()
            .position(|link| link.id == id)
            .map_or(0, |index| index + 1)
    }

    /// Shared shaders may occur in several submissions. All earlier snapshots
    /// must reach ANGLE before the shader's source or compiled state changes.
    pub fn through_shader(&self, id: u32) -> usize {
        self.entries
            .iter()
            .rposition(|link| link.shaders.iter().any(|shader| shader.id == id))
            .map_or(0, |index| index + 1)
    }
}

#[cfg(test)]
mod tests;
