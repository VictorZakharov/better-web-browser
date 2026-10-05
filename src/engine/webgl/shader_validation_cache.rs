//! Per-native-context successful WebGL compiler-output cache.
//! This caches neither GL shader objects nor link/compile status: native
//! compilation still runs for every compileShader, including cache hits.

use super::ApiVersion;
use std::collections::VecDeque;

const MAX_ENTRIES: usize = 16;
const MAX_BYTES: usize = 4 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Environment {
    pub api: ApiVersion,
    pub kind: u32,
    pub derivatives: bool,
    pub frag_depth: bool,
    pub texture_lod: bool,
    pub draw_buffers: bool,
    pub max_draw_buffers: u32,
}

struct Entry {
    environment: Environment,
    source: String,
    translated: String,
}

impl Entry {
    fn bytes(&self) -> usize {
        self.source.len() + self.translated.len()
    }
}

#[derive(Default)]
pub(super) struct Cache {
    entries: VecDeque<Entry>,
    bytes: usize,
    #[cfg(test)]
    pub hits: usize,
}

impl Cache {
    pub fn lookup(&mut self, environment: Environment, source: &str) -> Option<String> {
        let index = self
            .entries
            .iter()
            .position(|entry| entry.environment == environment && entry.source == source)?;
        // Exact owned source comparison avoids hash collisions and author hooks.
        // Move the matching entry to the end; eviction always removes the LRU.
        let entry = self.entries.remove(index)?;
        let translated = entry.translated.clone();
        self.entries.push_back(entry);
        #[cfg(test)]
        {
            self.hits += 1;
        }
        Some(translated)
    }

    pub fn insert(&mut self, environment: Environment, source: &str, translated: &str) {
        let Some(bytes) = source
            .len()
            .checked_add(translated.len())
            .filter(|v| *v <= MAX_BYTES)
        else {
            // A valid large shader still compiles; caching is not admission.
            return;
        };
        if let Some(index) = self
            .entries
            .iter()
            .position(|entry| entry.environment == environment && entry.source == source)
            && let Some(previous) = self.entries.remove(index)
        {
            self.bytes -= previous.bytes();
        }
        while self.entries.len() >= MAX_ENTRIES || self.bytes > MAX_BYTES - bytes {
            let Some(previous) = self.entries.pop_front() else {
                break;
            };
            self.bytes -= previous.bytes();
        }
        self.entries.push_back(Entry {
            environment,
            source: source.to_owned(),
            translated: translated.to_owned(),
        });
        self.bytes += bytes;
    }
}

#[cfg(test)]
mod native_tests;
#[cfg(test)]
mod tests;
