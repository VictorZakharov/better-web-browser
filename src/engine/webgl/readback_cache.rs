//! A bounded owned copy of the last successful default-buffer color read.
//! Never cache author FBOs, pack-buffer writes, padding or out-of-bounds pixels.
const MAX_BYTES: usize = 64 * 1024;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) struct Region {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

#[derive(Default)]
pub(super) struct Cache {
    region: Option<Region>,
    bytes: Vec<u8>,
}

impl Cache {
    pub(super) fn read(&self, region: Region) -> Option<Vec<u8>> {
        (self.region == Some(region)).then(|| self.bytes.clone())
    }

    pub(super) fn invalidate(&mut self) {
        self.region = None;
        // Retain already charged capacity, but never expose the old pixels.
        self.bytes.clear();
    }

    pub(super) fn record(&mut self, region: Region, bytes: &[u8], available: usize) -> usize {
        self.invalidate();
        if bytes.is_empty() || bytes.len() > MAX_BYTES {
            return 0;
        }
        let previous = self.bytes.capacity();
        let growth = bytes.len().saturating_sub(previous);
        if growth > available / 2 {
            return 0;
        }
        if growth != 0 {
            let replacement = bytes.to_vec();
            // Vec capacity is allowed to exceed the requested length. Check
            // the actual allocation before retaining or charging any bytes.
            let charge = replacement
                .capacity()
                .saturating_sub(previous)
                .saturating_mul(2);
            if replacement.capacity() > MAX_BYTES || charge > available {
                return 0;
            }
            self.bytes = replacement;
        } else {
            self.bytes.extend_from_slice(bytes);
        }
        self.region = Some(region);
        (self.bytes.capacity() - previous) * 2
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const REGION: Region = Region {
        x: 0,
        y: 0,
        width: 1,
        height: 1,
    };

    #[test]
    fn replies_are_owned_and_only_the_exact_valid_region_is_reused() {
        let mut cache = Cache::default();
        assert_eq!(cache.record(REGION, &[1, 2, 3, 4], 8), 8);
        let mut reply = cache.read(REGION).unwrap();
        reply[0] = 9;
        assert_eq!(cache.read(REGION).unwrap(), [1, 2, 3, 4]);
        assert!(cache.read(Region { x: 1, ..REGION }).is_none());
        cache.invalidate();
        assert!(cache.read(REGION).is_none());
        assert_eq!(cache.record(REGION, &[4, 3, 2, 1], 0), 0);
        assert_eq!(cache.read(REGION).unwrap(), [4, 3, 2, 1]);
    }

    #[test]
    fn optional_cache_exhaustion_never_allocates_unbounded_retained_storage() {
        let mut cache = Cache::default();
        assert_eq!(cache.record(REGION, &[1; 4], 7), 0);
        assert!(cache.read(REGION).is_none());
        assert_eq!(cache.record(REGION, &vec![1; MAX_BYTES + 1], usize::MAX), 0);
        assert_eq!(cache.bytes.capacity(), 0);
        assert_eq!(
            cache.record(REGION, &vec![1; MAX_BYTES], MAX_BYTES * 2),
            MAX_BYTES * 2
        );
        assert_eq!(cache.record(REGION, &[2; 4], 0), 0);
        assert_eq!(cache.bytes.capacity(), MAX_BYTES);
    }
}
