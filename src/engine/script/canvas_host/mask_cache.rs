//! Reuse only pure coverage results, never a painted Canvas bitmap. Exact request
//! equality includes geometry, pen, transform, fill rule and raster region. Paint,
//! clipping and compositing are still evaluated separately on every draw.

use super::coverage_storage::Coverage;
use std::cell::RefCell;
use std::collections::VecDeque;
use std::sync::Arc;

// Compressed thin strokes can fit many more exact keys under the unchanged
// byte budget. Bound entry metadata as well as retained sample/key storage.
const MAX_ENTRIES: usize = 512;
const MAX_BYTES: usize = 8 * 1024 * 1024;
const MAX_KEY_BYTES: usize = 256 * 1024;
const MAX_MASK_BYTES: usize = 1024 * 1024;

thread_local! {
    // Renderer threads do not share mutable masks or synchronisation locks.
    static MASKS: RefCell<Cache> = RefCell::new(Cache::default());
}

#[cfg(test)]
mod compact_tests;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Kind {
    Fill,
    Stroke,
}

struct Entry {
    kind: Kind,
    request: String,
    geometry: Option<Vec<u8>>,
    mask: Arc<Coverage>,
}

impl Entry {
    fn bytes(&self) -> usize {
        self.request.len() + self.geometry.as_ref().map_or(0, Vec::len) + self.mask.bytes()
    }
}

#[derive(Default)]
struct Cache {
    entries: VecDeque<Entry>,
    bytes: usize,
}

impl Cache {
    fn get_shared(&mut self, kind: Kind, request: &str) -> Option<Arc<Coverage>> {
        self.get_payload(kind, request, None)
    }

    fn get_payload(
        &mut self,
        kind: Kind,
        request: &str,
        geometry: Option<&[u8]>,
    ) -> Option<Arc<Coverage>> {
        let index = self.entries.iter().position(|entry| {
            entry.kind == kind && entry.request == request && entry.geometry.as_deref() == geometry
        })?;
        let entry = self.entries.remove(index)?;
        // Native fused painters only read coverage. Sharing the immutable
        // allocation avoids a second full-mask copy on hits and insertions.
        let mask = Arc::clone(&entry.mask);
        self.entries.push_back(entry);
        Some(mask)
    }

    fn insert_shared(&mut self, kind: Kind, request: &str, mask: Arc<Coverage>) {
        self.insert_payload(kind, request, None, mask);
    }

    fn insert_payload(
        &mut self,
        kind: Kind,
        request: &str,
        geometry: Option<&[u8]>,
        mask: Arc<Coverage>,
    ) {
        let key_bytes = request
            .len()
            .saturating_add(geometry.map_or(0, <[u8]>::len));
        if key_bytes > MAX_KEY_BYTES || mask.len() > MAX_MASK_BYTES {
            return;
        }
        if let Some(index) = self.entries.iter().position(|entry| {
            entry.kind == kind && entry.request == request && entry.geometry.as_deref() == geometry
        }) {
            let previous = self.entries.remove(index).expect("located entry");
            self.bytes -= previous.bytes();
        }
        let needed = key_bytes + mask.bytes();
        while self.entries.len() >= MAX_ENTRIES || self.bytes + needed > MAX_BYTES {
            let Some(oldest) = self.entries.pop_front() else {
                return;
            };
            self.bytes -= oldest.bytes();
        }
        self.entries.push_back(Entry {
            kind,
            request: request.to_owned(),
            geometry: geometry.map(<[u8]>::to_vec),
            mask,
        });
        self.bytes += needed;
    }

    #[cfg(test)]
    fn get(&mut self, kind: Kind, request: &str) -> Option<Vec<u8>> {
        self.get_shared(kind, request)
            .map(|mask| Arc::unwrap_or_clone(mask).into_vec())
    }

    #[cfg(test)]
    fn insert(&mut self, kind: Kind, request: &str, mask: &[u8]) {
        self.insert_shared(kind, request, Arc::new(Coverage::new(mask.to_vec())));
    }
}

#[cfg(test)]
fn rasterize(
    kind: Kind,
    request: &str,
    render: impl FnOnce() -> Option<Vec<u8>>,
) -> Option<Vec<u8>> {
    // JavaScript receives owned bytes, never an alias of cached storage.
    rasterize_shared(kind, request, render).map(|mask| Arc::unwrap_or_clone(mask).into_vec())
}

pub(super) fn rasterize_shared(
    kind: Kind,
    request: &str,
    render: impl FnOnce() -> Option<Vec<u8>>,
) -> Option<Arc<Coverage>> {
    if request.len() <= MAX_KEY_BYTES
        && let Some(mask) = MASKS.with(|cache| cache.borrow_mut().get_shared(kind, request))
    {
        return Some(mask);
    }
    // Do not hold a RefCell borrow across rasterization. Unsupported/invalid
    // requests are never cached: their normal validation and fallback remain.
    let mask = Arc::new(Coverage::new(render()?));
    MASKS.with(|cache| {
        cache
            .borrow_mut()
            .insert_shared(kind, request, Arc::clone(&mask))
    });
    Some(mask)
}

pub(super) fn rasterize_packed(
    kind: Kind,
    request: &str,
    geometry: &[u8],
    render: impl FnOnce() -> Option<Vec<u8>>,
) -> Option<Arc<Coverage>> {
    if geometry.len() > super::packed_geometry::MAX_BYTES {
        return None;
    }
    let key_bytes = request.len().saturating_add(geometry.len());
    if key_bytes <= MAX_KEY_BYTES
        && let Some(mask) = MASKS.with(|cache| {
            cache
                .borrow_mut()
                .get_payload(kind, request, Some(geometry))
        })
    {
        return Some(mask);
    }
    // Exact independent metadata/byte equality: no hash collision or alias can
    // substitute another path. The existing cache budget counts both fields.
    let mask = Arc::new(Coverage::new(render()?));
    MASKS.with(|cache| {
        cache
            .borrow_mut()
            .insert_payload(kind, request, Some(geometry), Arc::clone(&mask))
    });
    Some(mask)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packed_key_storage_is_owned_and_counted_in_the_same_budget() {
        let mut cache = Cache::default();
        let mut geometry = vec![17; 1024];
        cache.insert_payload(
            Kind::Fill,
            "metadata",
            Some(&geometry),
            Arc::new(Coverage::new(vec![128; 16])),
        );
        assert_eq!(cache.bytes, 8 + 1024 + 16);
        geometry.fill(99);
        assert!(
            cache
                .get_payload(Kind::Fill, "metadata", Some(&geometry))
                .is_none()
        );
        assert!(
            cache
                .get_payload(Kind::Fill, "metadata", Some(&vec![17; 1024]))
                .is_some()
        );
        cache.insert_payload(
            Kind::Fill,
            "too big",
            Some(&vec![0; MAX_KEY_BYTES]),
            Arc::new(Coverage::new(vec![1])),
        );
        assert_eq!(cache.entries.len(), 1);
        for index in 0..MAX_ENTRIES {
            cache.insert_payload(
                Kind::Stroke,
                &index.to_string(),
                Some(&geometry),
                Arc::new(Coverage::new(vec![128; MAX_MASK_BYTES])),
            );
            assert!(cache.bytes <= MAX_BYTES);
            assert_eq!(
                cache.bytes,
                cache.entries.iter().map(Entry::bytes).sum::<usize>()
            );
        }
    }

    #[test]
    fn native_consumers_share_storage_without_aliasing_owned_js_results() {
        let key = "shared coverage ownership regression";
        let first = rasterize_shared(Kind::Stroke, key, || Some(vec![0, 127, 255])).unwrap();
        let second = rasterize_shared(Kind::Stroke, key, || panic!("cache hit")).unwrap();
        assert!(Arc::ptr_eq(&first, &second));
        let mut owned = rasterize(Kind::Stroke, key, || panic!("cache hit")).unwrap();
        owned.fill(99);
        assert_eq!(first.to_vec(), [0, 127, 255]);
        assert_eq!(second.to_vec(), [0, 127, 255]);
        let mut detached = Arc::unwrap_or_clone(second).into_vec();
        detached.fill(17);
        assert_eq!(first.to_vec(), [0, 127, 255]);
        let third = rasterize_shared(Kind::Stroke, key, || panic!("cache hit")).unwrap();
        assert!(Arc::ptr_eq(&first, &third));
    }

    #[test]
    fn cache_ownership_does_not_extend_accounted_storage_after_eviction() {
        let mut cache = Cache::default();
        let retained = Arc::new(Coverage::new(vec![128; MAX_MASK_BYTES]));
        cache.insert_shared(Kind::Fill, "retained", Arc::clone(&retained));
        assert_eq!(Arc::strong_count(&retained), 2);
        for index in 0..MAX_ENTRIES {
            cache.insert_shared(
                Kind::Fill,
                &index.to_string(),
                Arc::new(Coverage::new(vec![255; MAX_MASK_BYTES])),
            );
        }
        assert_eq!(Arc::strong_count(&retained), 1);
        assert!(cache.bytes <= MAX_BYTES);
        assert_eq!(retained.len(), MAX_MASK_BYTES);
    }

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
