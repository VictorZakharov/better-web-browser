//! Reuse source arithmetic, never backdrop pixels or geometry. Exact f64 bits
//! keep half-byte ties and signed-zero evaluation distinct. Uniform tables
//! cannot be substituted for masks containing fractional coverage values.
use super::Paint;
use std::cell::RefCell;
use std::collections::VecDeque;
use std::sync::Arc;

const MAX_ENTRIES: usize = 16;

#[derive(Clone, Copy, PartialEq, Eq)]
struct Key {
    channels: [u64; 4],
    opacity: u64,
    masked: bool,
}

impl Key {
    fn new(color: [f64; 4], opacity: f64, masked: bool) -> Self {
        Self {
            channels: color.map(f64::to_bits),
            opacity: opacity.to_bits(),
            masked,
        }
    }
}

#[derive(Default)]
struct Cache(VecDeque<(Key, Arc<Paint>)>);

impl Cache {
    fn get(&mut self, key: Key, create: impl FnOnce() -> Paint) -> Arc<Paint> {
        if let Some(index) = self.0.iter().position(|(candidate, _)| *candidate == key) {
            let entry = self.0.remove(index).expect("located arithmetic entry");
            let paint = Arc::clone(&entry.1);
            self.0.push_back(entry);
            return paint;
        }
        let paint = Arc::new(create());
        if self.0.len() == MAX_ENTRIES {
            self.0.pop_front();
        }
        self.0.push_back((key, Arc::clone(&paint)));
        paint
    }
}

thread_local! {
    // Fixed-size source tables: sixteen entries retain less than 256 KiB,
    // including source keys and Arc headers. Renderer threads share no locks.
    static CACHE: RefCell<Cache> = RefCell::new(Cache::default());
}

pub(super) fn get(color: [f64; 4], opacity: f64, masked: bool) -> Arc<Paint> {
    let key = Key::new(color, opacity, masked);
    CACHE.with(|cache| {
        cache.borrow_mut().get(key, || {
            Paint::for_mask(color, opacity, masked.then_some(&[]))
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cached_source_tables_match_every_scalar_coverage_and_keep_backdrops_private() {
        for color in [
            [31.5, 205.1, 73.25, 183.0],
            [255.0, 0.0, 127.5, 255.0],
            [0.0; 4],
        ] {
            for opacity in [0.0, 0.37, 1.0] {
                let first = get(color, opacity, true);
                let second = get(color, opacity, true);
                assert!(Arc::ptr_eq(&first, &second));
                for coverage in 0..=255 {
                    for alpha in [0, 1, 127, 255] {
                        let mut actual = [29, 63, 193, alpha];
                        let mut expected = actual;
                        first.apply(&mut actual, coverage);
                        super::super::super::source_over(
                            &mut expected,
                            color,
                            opacity * (f64::from(coverage) / 255.0),
                        );
                        assert_eq!(actual, expected);
                    }
                }
            }
        }
    }

    #[test]
    fn every_source_bit_and_mask_mode_is_part_of_the_exact_key() {
        let original = Key::new([1.0; 4], 0.5, true);
        for channel in 0..4 {
            let mut color = [1.0; 4];
            color[channel] = f64::from_bits(1.0_f64.to_bits() + 1);
            assert!(Key::new(color, 0.5, true) != original);
        }
        assert!(Key::new([1.0; 4], f64::from_bits(0.5_f64.to_bits() + 1), true) != original);
        assert!(Key::new([1.0; 4], 0.5, false) != original);
        assert!(Key::new([0.0; 4], 0.5, true) != Key::new([-0.0; 4], 0.5, true));
        let uniform = get([1.0; 4], 0.5, false);
        let masked = get([1.0; 4], 0.5, true);
        assert!(!Arc::ptr_eq(&uniform, &masked));
        let mut a = [29, 63, 193, 127];
        let mut b = a;
        uniform.apply(&mut a, 255);
        masked.apply(&mut b, 255);
        assert_eq!(a, b);
    }

    #[test]
    fn bounded_lru_eviction_does_not_change_a_retained_table() {
        let mut cache = Cache::default();
        let key = Key::new([31.5, 205.1, 73.25, 183.0], 0.37, true);
        let first = cache.get(key, || Paint::new([31.5, 205.1, 73.25, 183.0], 0.37));
        for index in 0..MAX_ENTRIES {
            cache.get(Key::new([index as f64; 4], 1.0, true), || {
                Paint::new([index as f64; 4], 1.0)
            });
        }
        assert_eq!(cache.0.len(), MAX_ENTRIES);
        assert!(cache.0.iter().all(|(candidate, _)| *candidate != key));
        let mut pixel = [29, 63, 193, 127];
        let mut expected = pixel;
        first.apply(&mut pixel, 128);
        super::super::super::source_over(
            &mut expected,
            [31.5, 205.1, 73.25, 183.0],
            0.37 * (128.0 / 255.0),
        );
        assert_eq!(pixel, expected);
        let entry_bytes = std::mem::size_of::<Paint>()
            + std::mem::size_of::<Key>()
            + std::mem::size_of::<Arc<Paint>>()
            + 2 * std::mem::size_of::<usize>();
        assert!(MAX_ENTRIES * entry_bytes < 256 * 1024);
    }
}
