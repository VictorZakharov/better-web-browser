//! Fixed-count derived index ranges. No cached vertex, VAO or draw admission.
use std::collections::VecDeque;

const MAX_RANGES: usize = 64;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Key {
    pub owner: u32,
    pub offset: usize,
    pub count: usize,
    pub width: usize,
}

#[derive(Clone, Copy)]
struct Entry {
    key: Key,
    maximum: Option<u32>,
}

#[derive(Default)]
pub(super) struct Cache {
    entries: VecDeque<Entry>,
    #[cfg(test)]
    pub(super) scanned_bytes: usize,
}

impl Cache {
    // Outer None means a miss. Inner None is a genuine all-restart index range.
    pub fn lookup(&mut self, key: Key) -> Option<Option<u32>> {
        let index = self.entries.iter().position(|entry| entry.key == key)?;
        let entry = self.entries.remove(index)?;
        self.entries.push_back(entry);
        Some(entry.maximum)
    }

    pub fn insert(&mut self, key: Key, maximum: Option<u32>) {
        if let Some(index) = self.entries.iter().position(|entry| entry.key == key) {
            self.entries.remove(index);
        }
        if self.entries.len() == MAX_RANGES {
            self.entries.pop_front();
        }
        self.entries.push_back(Entry { key, maximum });
    }

    // A successful write invalidates every range for this buffer, including
    // disjoint ranges. Partial-range preservation is not needed for correctness.
    pub fn remove(&mut self, owner: u32) {
        self.entries.retain(|entry| entry.key.owner != owner);
    }
}

#[cfg(test)]
mod tests;
