//! Cache only the pure Gaussian alpha mask. Paint RGB, offset, global operator
//! and final drawing clip are never retained; each draw samples/composites anew.
//! Exact key equality guards hash collisions and source alpha mutations.

use image::{ImageBuffer, Luma};
use std::cell::RefCell;
use std::collections::VecDeque;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::sync::Arc;

type Alpha = ImageBuffer<Luma<f32>, Vec<f32>>;
const MAX_BYTES: usize = 16 * 1024 * 1024;
const MAX_ENTRY_BYTES: usize = 8 * 1024 * 1024;
const MAX_ENTRIES: usize = 8;
const MIN_MASK_PIXELS: usize = 64 * 1024;

#[derive(PartialEq, Eq, Hash)]
struct Key {
    width: u32,
    height: u32,
    padding: u32,
    sigma: u64,
    source: Vec<u8>,
}

impl Key {
    fn hash(&self) -> u64 {
        let mut hash = DefaultHasher::new();
        Hash::hash(self, &mut hash);
        hash.finish()
    }
}

struct Entry {
    hash: u64,
    key: Key,
    alpha: Arc<Alpha>,
    bytes: usize,
}

#[derive(Default)]
struct Cache {
    entries: VecDeque<Entry>,
    bytes: usize,
}

impl Cache {
    fn get(&mut self, hash: u64, key: &Key) -> Option<Arc<Alpha>> {
        let index = self
            .entries
            .iter()
            .position(|entry| entry.hash == hash && entry.key == *key)?;
        let entry = self.entries.remove(index)?;
        let alpha = Arc::clone(&entry.alpha);
        self.entries.push_back(entry);
        Some(alpha)
    }

    fn insert(&mut self, hash: u64, key: Key, alpha: Arc<Alpha>) {
        let bytes = key.source.len() + alpha.as_raw().len() * std::mem::size_of::<f32>();
        if bytes > MAX_ENTRY_BYTES {
            return;
        }
        while self.entries.len() >= MAX_ENTRIES || self.bytes + bytes > MAX_BYTES {
            let Some(oldest) = self.entries.pop_front() else {
                return;
            };
            self.bytes -= oldest.bytes;
        }
        self.entries.push_back(Entry {
            hash,
            key,
            alpha,
            bytes,
        });
        self.bytes += bytes;
    }
}

thread_local! {
    // Pure immutable results are private to the renderer/worker execution thread.
    // The cache never owns a realm, Canvas object, or author-visible typed array.
    static KERNELS: RefCell<Cache> = RefCell::new(Cache::default());
}

pub(super) fn gaussian(
    source: Vec<u8>,
    width: u32,
    height: u32,
    padding: u32,
    sigma: f64,
) -> Arc<Alpha> {
    let key = Key {
        width,
        height,
        padding,
        sigma: sigma.to_bits(),
        source,
    };
    let mask_width = width + padding * 2;
    let mask_height = height + padding * 2;
    let cacheable = mask_width as usize * mask_height as usize >= MIN_MASK_PIXELS && sigma >= 0.01;
    let hash = if cacheable { key.hash() } else { 0 };
    if cacheable && let Some(alpha) = KERNELS.with(|cache| cache.borrow_mut().get(hash, &key)) {
        return alpha;
    }
    let mut alpha = Alpha::new(mask_width, mask_height);
    for (y, row) in key.source.chunks_exact(width as usize).enumerate() {
        for (x, value) in row.iter().enumerate() {
            alpha.put_pixel(
                x as u32 + padding,
                y as u32 + padding,
                Luma([f32::from(*value) / 255.0]),
            );
        }
    }
    // Keep the existing image-rs Gaussian and its arithmetic exactly. A cache
    // is not permission to substitute a cheaper approximate blur kernel.
    let alpha = Arc::new(if sigma < 0.01 {
        alpha
    } else {
        image::imageops::blur(&alpha, sigma as f32)
    });
    if cacheable {
        KERNELS.with(|cache| cache.borrow_mut().insert(hash, key, Arc::clone(&alpha)));
    }
    alpha
}

#[cfg(test)]
mod tests;
