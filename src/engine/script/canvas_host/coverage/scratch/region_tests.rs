use super::*;

#[test]
fn dirty_rectangle_clear_keeps_every_other_sample_zero() {
    SCRATCH.with(|slot| *slot.borrow_mut() = None);
    let mut mask = take(97, 81).unwrap();
    let pointer = mask.data().as_ptr();
    for y in 13..17 {
        mask.data_mut()[y * 97 + 7..y * 97 + 31].fill(177);
    }
    retain_region(mask, [7, 13, 24, 4]);
    let reused = take(97, 81).unwrap();
    assert_eq!(reused.data().as_ptr(), pointer);
    assert!(reused.data().iter().all(|v| *v == 0));
}

#[test]
fn malformed_internal_bounds_fall_back_to_full_clearing() {
    for dirty in [[96, 80, 2, 2], [u32::MAX, 0, 10, 1], [0, u32::MAX, 1, 10]] {
        let mut mask = take(97, 81).unwrap();
        mask.data_mut().fill(177);
        retain_region(mask, dirty);
        assert!(take(97, 81).unwrap().data().iter().all(|v| *v == 0));
    }
}

#[test]
fn alternating_support_never_reintroduces_samples_from_an_earlier_path() {
    for index in 0..30 {
        let mut mask = take(97, 81).unwrap();
        assert!(mask.data().iter().all(|v| *v == 0));
        let (left, top) = if index % 2 == 0 {
            (2usize, 7usize)
        } else {
            (81, 63)
        };
        for row in top..top + 11 {
            mask.data_mut()[row * 97 + left..row * 97 + left + 13].fill(index + 1);
        }
        retain_region(mask, [left as u32, top as u32, 13, 11]);
    }
}

#[test]
fn empty_dirty_region_can_reuse_an_untouched_zero_surface() {
    let mask = Mask::new(97, 81).unwrap();
    let pointer = mask.data().as_ptr();
    retain_region(mask, [97, 81, 0, 0]);
    let mask = take(97, 81).unwrap();
    assert_eq!(mask.data().as_ptr(), pointer);
    assert!(mask.data().iter().all(|v| *v == 0));
}
