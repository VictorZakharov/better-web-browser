//! Bounded metadata from real linked-program reflection, never cached locations or values.
use std::collections::VecDeque;

const MAX_PROGRAMS: usize = 32;
const MAX_BYTES: usize = 1024 * 1024;

struct Uniform {
    family: String,
    kind: u32,
}

pub(super) struct Table {
    uniforms: Vec<Uniform>,
    bytes: usize,
}

impl Table {
    fn find(&self, family: &str) -> Option<u32> {
        self.uniforms
            .binary_search_by(|uniform| uniform.family.as_str().cmp(family))
            .ok()
            .map(|index| self.uniforms[index].kind)
    }
}

/// Staging is independently bounded. Declining a cache must not reject a legal
/// uniform or change its type; the caller continues querying native reflection.
pub(super) struct Builder {
    table: Table,
}

impl Builder {
    pub fn new(count: usize) -> Option<Self> {
        let base = count.checked_mul(std::mem::size_of::<Uniform>())?;
        if base > MAX_BYTES {
            return None;
        }
        let uniforms = Vec::with_capacity(count);
        let bytes = uniforms.capacity() * std::mem::size_of::<Uniform>();
        (bytes <= MAX_BYTES).then_some(Self {
            table: Table { uniforms, bytes },
        })
    }

    pub fn push(&mut self, family: String, kind: u32) -> bool {
        if self.table.uniforms.len() == self.table.uniforms.capacity() {
            return false;
        }
        let Some(bytes) = self.table.bytes.checked_add(family.capacity()) else {
            return false;
        };
        if bytes > MAX_BYTES {
            return false;
        }
        self.table.bytes = bytes;
        self.table.uniforms.push(Uniform { family, kind });
        true
    }

    pub fn finish(mut self) -> Table {
        // Stable order keeps the first native occurrence of an array family,
        // matching the original scan. Array-of-struct fields retain their names.
        self.table.uniforms.sort_by(|a, b| a.family.cmp(&b.family));
        self.table.uniforms.dedup_by(|a, b| a.family == b.family);
        // The accounting deliberately retains the pre-dedup high water, so
        // allocator capacity and duplicate staging never create extra admission.
        self.table
    }
}

struct Entry {
    owner: u32,
    generation: u32,
    table: Table,
}

#[derive(Default)]
pub(super) struct Cache {
    entries: VecDeque<Entry>,
    bytes: usize,
    #[cfg(test)]
    pub(super) native_scans: usize,
}

impl Cache {
    /// Outer None is a miss; inner None is an actually reflected absent family.
    pub fn lookup(&mut self, owner: u32, generation: u32, family: &str) -> Option<Option<u32>> {
        let index = self
            .entries
            .iter()
            .position(|entry| entry.owner == owner && entry.generation == generation)?;
        let entry = self.entries.remove(index)?;
        let result = entry.table.find(family);
        self.entries.push_back(entry);
        Some(result)
    }

    pub fn remove(&mut self, owner: u32) {
        if let Some(index) = self.entries.iter().position(|entry| entry.owner == owner)
            && let Some(previous) = self.entries.remove(index)
        {
            self.bytes -= previous.table.bytes;
        }
    }

    pub fn insert(&mut self, owner: u32, generation: u32, table: Table) {
        self.remove(owner);
        debug_assert!(table.bytes <= MAX_BYTES);
        while self.entries.len() >= MAX_PROGRAMS || self.bytes > MAX_BYTES - table.bytes {
            let Some(previous) = self.entries.pop_front() else {
                break;
            };
            self.bytes -= previous.table.bytes;
        }
        self.bytes += table.bytes;
        self.entries.push_back(Entry {
            owner,
            generation,
            table,
        });
    }
}

#[cfg(test)]
mod tests;
