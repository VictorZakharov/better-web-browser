//! Linked vertex-input metadata only; buffer/VAO/range validation stays live.
use std::collections::VecDeque;

const MAX_PROGRAMS: usize = 32;

#[derive(Clone, Copy)]
struct Entry {
    owner: u32,
    generation: u32,
    active: u32,
}

#[derive(Default)]
pub(super) struct Cache {
    entries: VecDeque<Entry>,
    #[cfg(test)]
    pub(super) native_scans: usize,
}

impl Cache {
    pub fn lookup(&mut self, owner: u32, generation: u32) -> Option<u32> {
        let index = self
            .entries
            .iter()
            .position(|entry| entry.owner == owner && entry.generation == generation)?;
        let entry = self.entries.remove(index)?;
        self.entries.push_back(entry);
        Some(entry.active)
    }

    pub fn remove(&mut self, owner: u32) {
        self.entries.retain(|entry| entry.owner != owner);
    }

    pub fn insert(&mut self, owner: u32, generation: u32, active: u32) {
        self.remove(owner);
        if self.entries.len() == MAX_PROGRAMS {
            self.entries.pop_front();
        }
        self.entries.push_back(Entry {
            owner,
            generation,
            active,
        });
    }
}

/// Matrices consume one slot per column; attribute arrays repeat those slots.
/// No wrapping or unchecked shift can turn native metadata into a missing bound.
pub(super) fn slots(location: i32, kind: u32, size: i32, maximum: usize) -> Option<u32> {
    if location < 0 {
        return Some(0); // Reflected built-ins have no author attribute location.
    }
    let columns: usize = match kind {
        0x8b5a | 0x8b65 | 0x8b66 => 2,
        0x8b5b | 0x8b67 | 0x8b68 => 3,
        0x8b5c | 0x8b69 | 0x8b6a => 4,
        _ => 1,
    };
    let count = columns.checked_mul(usize::try_from(size).ok()?)?;
    let start = usize::try_from(location).ok()?;
    let end = start.checked_add(count)?;
    if count == 0 || maximum > 32 || end > maximum {
        return None;
    }
    Some((start..end).fold(0, |mask, index| mask | (1u32 << index)))
}

#[cfg(test)]
mod tests;
