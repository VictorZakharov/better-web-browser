//! Fixed-size Bloom keys: collisions only admit extra full selector matches, never reject one.
#[derive(Clone, Copy, Default)]
pub(super) struct Keys([u64; 4]);

impl Keys {
    pub(super) fn insert(&mut self, value: &str, ascii_insensitive: bool) {
        for bit in positions(value, ascii_insensitive) {
            self.0[bit / 64] |= 1 << (bit % 64);
        }
    }

    pub(super) fn contains(&self, value: &str, ascii_insensitive: bool) -> bool {
        positions(value, ascii_insensitive)
            .into_iter()
            .all(|bit| self.0[bit / 64] & (1 << (bit % 64)) != 0)
    }
}

fn positions(value: &str, ascii_insensitive: bool) -> [usize; 2] {
    // A non-cryptographic hash is sufficient for this rejection-only filter. Even an adversarial
    // collision falls back to the same complete matching path as a missing filter.
    let mut hash = 0xcbf29ce484222325_u64;
    for byte in value.bytes() {
        let byte = if ascii_insensitive {
            byte.to_ascii_lowercase()
        } else {
            byte
        };
        hash = (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3);
    }
    [(hash & 255) as usize, ((hash >> 32) & 255) as usize]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saturated_keys_never_reject_an_inserted_name() {
        let mut keys = Keys::default();
        for index in 0..10_000 {
            keys.insert(&format!("key-{index}"), false);
        }
        for index in 0..10_000 {
            assert!(keys.contains(&format!("key-{index}"), false));
        }
    }

    #[test]
    fn attribute_keys_fold_only_ascii_case() {
        let mut keys = Keys::default();
        keys.insert("DATA-É", true);
        assert!(keys.contains("data-É", true));
        assert!(!keys.contains("data-é", true));
    }
}
