//! Pure checked address policy; these values neither reserve nor commit stack pages.
const KIB: usize = 1024;
// Leave space for Rust/Win32/native bindings and V8 compiler/builtin frames.
const NATIVE_RESERVE: usize = 256 * KIB;
const MIN_EXECUTION_SPACE: usize = 64 * KIB;

// The pinned V8 152.2.0 globals.h default. Never enlarge its JS stack allowance
// merely because Rust workers or the GPU owner have larger OS stacks.
#[cfg(target_arch = "x86")]
const MAX_EXECUTION_SPACE: usize = 472 * KIB;
#[cfg(target_arch = "arm")]
const MAX_EXECUTION_SPACE: usize = 864 * KIB;
#[cfg(not(any(target_arch = "x86", target_arch = "arm")))]
const MAX_EXECUTION_SPACE: usize = 984 * KIB;

pub(super) fn boundary(low: usize, high: usize, current: usize) -> Option<usize> {
    if low == 0 || low >= high || current <= low || current >= high {
        return None;
    }
    let native_floor = low.checked_add(NATIVE_RESERVE)?;
    let default_floor = current.saturating_sub(MAX_EXECUTION_SPACE);
    let boundary = native_floor.max(default_floor).checked_add(3)? & !3;
    (current.checked_sub(boundary)? >= MIN_EXECUTION_SPACE).then_some(boundary)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_mib_main_stack_keeps_native_headroom_after_embedder_frames() {
        let low = 4 * 1024 * 1024;
        let high = low + 1024 * 1024;
        let current = high - 100 * KIB;
        let limit = boundary(low, high, current).unwrap();
        assert_eq!(limit, low + NATIVE_RESERVE);
        assert!(limit > current.saturating_sub(MAX_EXECUTION_SPACE));
        assert_eq!(current - limit, 668 * KIB);
    }

    #[test]
    fn large_worker_stack_does_not_increase_v8_default_recursion_allowance() {
        let low = 4 * 1024 * 1024;
        let high = low + 8 * 1024 * 1024;
        let current = high - 32 * KIB;
        let limit = boundary(low, high, current).unwrap();
        assert!(current - limit <= MAX_EXECUTION_SPACE);
        assert!(limit >= low + NATIVE_RESERVE);
    }

    #[test]
    fn half_mib_stack_has_bounded_execution_room_and_native_reserve() {
        let low = 1024 * 1024;
        let high = low + 512 * KIB;
        let limit = boundary(low, high, high - 32 * KIB).unwrap();
        assert_eq!(limit, low + NATIVE_RESERVE);
        assert_eq!(high - 32 * KIB - limit, 224 * KIB);
    }

    #[test]
    fn invalid_or_exhausted_regions_fail_instead_of_wrapping_addresses() {
        for (low, high, current) in [
            (0, 1024, 512),
            (20, 10, 15),
            (10, 20, 10),
            (10, 20, 20),
            (10, 20, 30),
            (usize::MAX - 10, usize::MAX, usize::MAX - 1),
            (1024, 1024 + 300 * KIB, 1024 + 300 * KIB - 1),
        ] {
            assert!(
                boundary(low, high, current).is_none(),
                "{low}/{high}/{current}"
            );
        }
    }

    #[test]
    fn alignment_only_raises_boundary_and_minimum_is_inclusive() {
        let low = 1024 * 1024 + 1;
        let high = low + 1024 * 1024;
        let floor = (low + NATIVE_RESERVE + 3) & !3;
        assert!(boundary(low, high, floor + MIN_EXECUTION_SPACE - 1).is_none());
        assert_eq!(
            boundary(low, high, floor + MIN_EXECUTION_SPACE),
            Some(floor)
        );
        assert_eq!(floor % 4, 0);
    }
}
