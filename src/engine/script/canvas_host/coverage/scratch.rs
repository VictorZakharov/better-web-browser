//! One bounded per-renderer scratch allocation. This is not a result cache:
//! every checkout clears all samples before tiny-skia paints a new path.
use resvg::tiny_skia::Mask;
use std::cell::RefCell;

const MAX_RETAINED_BYTES: usize = 8 * 1024 * 1024;

thread_local! {
    static SCRATCH: RefCell<Option<Mask>> = const { RefCell::new(None) };
}

pub(super) fn take(width: u32, height: u32) -> Option<Mask> {
    // Release the cell borrow before rendering. Nested use can allocate its own
    // surface without borrowing or overwriting the active rasterizer's mask.
    let reusable = SCRATCH.with(|slot| slot.borrow_mut().take());
    if let Some(mut mask) = reusable
        && mask.width() == width
        && mask.height() == height
    {
        mask.data_mut().fill(0);
        return Some(mask);
    }
    Mask::new(width, height)
}

pub(super) fn retain(mask: Mask) {
    if mask.data().len() <= MAX_RETAINED_BYTES {
        SCRATCH.with(|slot| *slot.borrow_mut() = Some(mask));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn clear() {
        SCRATCH.with(|slot| *slot.borrow_mut() = None);
    }

    #[test]
    fn same_shape_reuses_storage_but_never_previous_coverage() {
        clear();
        let mut first = take(71, 39).unwrap();
        let pointer = first.data().as_ptr();
        first.data_mut().fill(255);
        retain(first);
        let second = take(71, 39).unwrap();
        assert_eq!(second.data().as_ptr(), pointer);
        assert!(second.data().iter().all(|sample| *sample == 0));
    }

    #[test]
    fn equal_byte_count_does_not_reuse_an_incompatible_stride() {
        clear();
        let mut first = take(64, 32).unwrap();
        first.data_mut().fill(177);
        retain(first);
        let second = take(32, 64).unwrap();
        assert_eq!((second.width(), second.height()), (32, 64));
        assert!(second.data().iter().all(|sample| *sample == 0));
    }

    #[test]
    fn active_and_nested_rasters_cannot_alias() {
        clear();
        let mut active = take(20, 20).unwrap();
        active.data_mut()[27] = 123;
        let nested = take(20, 20).unwrap();
        assert_ne!(active.data().as_ptr(), nested.data().as_ptr());
        retain(nested);
        assert_eq!(active.data()[27], 123);
        retain(active);
        assert!(
            take(20, 20)
                .unwrap()
                .data()
                .iter()
                .all(|sample| *sample == 0)
        );
    }

    #[test]
    fn oversized_allocations_are_not_retained() {
        clear();
        retain(Mask::new(4096, 4096).unwrap());
        assert!(SCRATCH.with(|slot| slot.borrow().is_none()));
        assert!(take(0, 10).is_none());
        assert!(SCRATCH.with(|slot| slot.borrow().is_none()));
    }
}
