//! Reuse only pure coverage results, never a painted Canvas bitmap. Exact request
//! equality includes geometry, pen, transform, fill rule and raster region. Paint,
//! clipping and compositing are still evaluated separately on every draw.

use std::cell::RefCell;
use std::collections::VecDeque;

const MAX_ENTRIES: usize = 32;
const MAX_BYTES: usize = 8 * 1024 * 1024;
const MAX_KEY_BYTES: usize = 256 * 1024;
const MAX_MASK_BYTES: usize = 1024 * 1024;

thread_local! {
    // Renderer threads do not share mutable masks or synchronisation locks.
    static MASKS: RefCell<Cache> = RefCell::new(Cache::default());
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Kind {
    Fill,
    Stroke,
}

struct Entry {
    kind: Kind,
    request: String,
    mask: Vec<u8>,
}

impl Entry {
    fn bytes(&self) -> usize {
        self.request.len() + self.mask.len()
    }
}

#[derive(Default)]
struct Cache {
    entries: VecDeque<Entry>,
    bytes: usize,
}

impl Cache {
    fn get(&mut self, kind: Kind, request: &str) -> Option<Vec<u8>> {
        let index = self
            .entries
            .iter()
            .position(|entry| entry.kind == kind && entry.request == request)?;
        let entry = self.entries.remove(index)?;
        // Each host call transfers its own bytes to JavaScript. A caller cannot
        // mutate the cached coverage through a previous returned typed array.
        let mask = entry.mask.clone();
        self.entries.push_back(entry);
        Some(mask)
    }

    fn insert(&mut self, kind: Kind, request: &str, mask: &[u8]) {
        if request.len() > MAX_KEY_BYTES || mask.len() > MAX_MASK_BYTES {
            return;
        }
        if let Some(index) = self
            .entries
            .iter()
            .position(|entry| entry.kind == kind && entry.request == request)
        {
            let previous = self.entries.remove(index).expect("located entry");
            self.bytes -= previous.bytes();
        }
        let needed = request.len() + mask.len();
        while self.entries.len() >= MAX_ENTRIES || self.bytes + needed > MAX_BYTES {
            let Some(oldest) = self.entries.pop_front() else {
                return;
            };
            self.bytes -= oldest.bytes();
        }
        self.entries.push_back(Entry {
            kind,
            request: request.to_owned(),
            mask: mask.to_vec(),
        });
        self.bytes += needed;
    }
}

pub(super) fn rasterize(
    kind: Kind,
    request: &str,
    render: impl FnOnce() -> Option<Vec<u8>>,
) -> Option<Vec<u8>> {
    if request.len() <= MAX_KEY_BYTES
        && let Some(mask) = MASKS.with(|cache| cache.borrow_mut().get(kind, request))
    {
        return Some(mask);
    }
    // Do not hold a RefCell borrow across rasterization. Unsupported/invalid
    // requests are never cached: their normal validation and fallback remain.
    let mask = render()?;
    MASKS.with(|cache| cache.borrow_mut().insert(kind, request, &mask));
    Some(mask)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_request_and_operation_are_both_required() {
        let mut cache = Cache::default();
        cache.insert(Kind::Stroke, "geometry", &[0, 128, 255]);
        assert_eq!(cache.get(Kind::Stroke, "geometry"), Some(vec![0, 128, 255]));
        assert!(cache.get(Kind::Fill, "geometry").is_none());
        assert!(cache.get(Kind::Stroke, "other geometry").is_none());
    }

    #[test]
    fn returned_bytes_cannot_modify_retained_coverage() {
        let mut cache = Cache::default();
        cache.insert(Kind::Fill, "path", &[128]);
        let mut returned = cache.get(Kind::Fill, "path").unwrap();
        returned[0] = 0;
        assert_eq!(cache.get(Kind::Fill, "path"), Some(vec![128]));
    }

    #[test]
    fn lookup_promotes_and_entry_limit_evicts_least_recently_used() {
        let mut cache = Cache::default();
        for index in 0..MAX_ENTRIES {
            cache.insert(Kind::Fill, &index.to_string(), &[1]);
        }
        assert!(cache.get(Kind::Fill, "0").is_some());
        cache.insert(Kind::Fill, "next", &[2]);
        assert!(cache.get(Kind::Fill, "1").is_none());
        assert!(cache.get(Kind::Fill, "0").is_some());
        assert_eq!(cache.entries.len(), MAX_ENTRIES);
    }

    #[test]
    fn byte_budget_counts_keys_and_masks_and_replacement() {
        let mut cache = Cache::default();
        let mask = vec![255; MAX_MASK_BYTES];
        for index in 0..20 {
            cache.insert(Kind::Stroke, &index.to_string(), &mask);
            assert!(cache.bytes <= MAX_BYTES);
        }
        assert_eq!(cache.entries.len(), 7);
        let before = cache.bytes;
        cache.insert(Kind::Stroke, "19", &[128]);
        assert_eq!(cache.bytes, before - MAX_MASK_BYTES + 1);
        assert_eq!(cache.get(Kind::Stroke, "19"), Some(vec![128]));
        assert_eq!(
            cache.bytes,
            cache.entries.iter().map(Entry::bytes).sum::<usize>()
        );
    }

    #[test]
    fn oversized_successes_bypass_storage_without_evicting_small_entries() {
        let mut cache = Cache::default();
        cache.insert(Kind::Fill, "small", &[1]);
        cache.insert(Kind::Fill, &"x".repeat(MAX_KEY_BYTES + 1), &[1]);
        cache.insert(Kind::Fill, "large mask", &vec![0; MAX_MASK_BYTES + 1]);
        assert_eq!(cache.entries.len(), 1);
        assert_eq!(cache.get(Kind::Fill, "small"), Some(vec![1]));
    }

    #[test]
    fn only_successful_results_are_reused_and_empty_masks_are_successful() {
        let key = "mask-cache failure retry test";
        assert!(rasterize(Kind::Fill, key, || None).is_none());
        assert_eq!(rasterize(Kind::Fill, key, || Some(vec![])), Some(vec![]));
        assert_eq!(
            rasterize(Kind::Fill, key, || panic!(
                "should reuse successful empty mask"
            )),
            Some(vec![])
        );
    }
}
