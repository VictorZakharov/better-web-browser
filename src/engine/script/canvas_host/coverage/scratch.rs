//! One bounded per-renderer scratch allocation. This is not a result cache:
//! every checkout clears the prior paint's conservative support before reuse.
use resvg::tiny_skia::Mask;
use std::cell::RefCell;

const MAX_RETAINED_BYTES: usize = 8 * 1024 * 1024;

thread_local! {
    static SCRATCH: RefCell<Option<Surface>> = const { RefCell::new(None) };
}

struct Surface {
    mask: Mask,
    dirty: [u32; 4],
}

pub(super) fn take(width: u32, height: u32) -> Option<Mask> {
    // Release the cell borrow before rendering. Nested use can allocate its own
    // surface without borrowing or overwriting the active rasterizer's mask.
    let reusable = SCRATCH.with(|slot| slot.borrow_mut().take());
    if let Some(Surface { mut mask, dirty }) = reusable
        && mask.width() == width
        && mask.height() == height
    {
        let [left, top, columns, rows] = dirty;
        for row in top..top + rows {
            let start = row as usize * width as usize + left as usize;
            mask.data_mut()[start..start + columns as usize].fill(0);
        }
        return Some(mask);
    }
    Mask::new(width, height)
}

#[cfg(test)]
pub(super) fn retain(mask: Mask) {
    let dirty = [0, 0, mask.width(), mask.height()];
    retain_region(mask, dirty);
}

/// `dirty` encloses every sample written by fill_path, not just nonzero reduced
/// pixels. Region::rasterize supplies control-point bounds plus a bitmap-pixel
/// AA margin, in the original full surface's scaled coordinate space.
pub(super) fn retain_region(mask: Mask, dirty: [u32; 4]) {
    if mask.data().len() <= MAX_RETAINED_BYTES {
        let [left, top, columns, rows] = dirty;
        let valid = left.checked_add(columns).is_some_and(|v| v <= mask.width())
            && top.checked_add(rows).is_some_and(|v| v <= mask.height());
        // A malformed internal bound must never leak old coverage on reuse.
        let dirty = if valid {
            dirty
        } else {
            [0, 0, mask.width(), mask.height()]
        };
        #[cfg(debug_assertions)]
        for (index, sample) in mask.data().iter().enumerate() {
            let x = index as u32 % mask.width();
            let y = index as u32 / mask.width();
            debug_assert!(
                *sample == 0
                    || (x >= dirty[0]
                        && x - dirty[0] < dirty[2]
                        && y >= dirty[1]
                        && y - dirty[1] < dirty[3]),
                "scratch bounds missed a painted sample"
            );
        }
        SCRATCH.with(|slot| *slot.borrow_mut() = Some(Surface { mask, dirty }));
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

#[cfg(test)]
#[path = "scratch/region_tests.rs"]
mod region_tests;
