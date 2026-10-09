//! Private commit is an extra admission signal, not a replacement for GPU
//! accounting or the broker's hard Job. Leave room for JS, compiler and IPC work.
const UNKNOWN_SAMPLE_CEILING: usize = 64 * 1024 * 1024;

pub(super) fn available(ceiling: usize, private: Option<usize>) -> usize {
    available_for(crate::renderer_budget::current(), ceiling, private)
}

fn available_for(
    budget: crate::renderer_budget::RendererBudget,
    ceiling: usize,
    private: Option<usize>,
) -> usize {
    let limit = budget.bytes();
    match private {
        Some(private) => ceiling.min(limit.saturating_sub(limit / 8).saturating_sub(private)),
        None => ceiling.min(UNKNOWN_SAMPLE_CEILING),
    }
}

pub(super) fn admits(charged: usize, growth: usize, private: Option<usize>) -> bool {
    if growth == 0 {
        return true;
    }
    match private {
        Some(_) => growth <= available(usize::MAX, private),
        // Retain the previous conservative baseline on unsupported/failed
        // observations. Unknown cannot justify the larger HDR storage budget.
        None => charged
            .checked_add(growth)
            .is_some_and(|next| next <= UNKNOWN_SAMPLE_CEILING),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::limits::RENDERER_MEMORY_LIMIT_BYTES;
    const MIB: usize = 1024 * 1024;

    #[test]
    fn graphics_mode_uses_its_selected_limit_and_the_same_reserve_fraction() {
        use crate::renderer_budget::RendererBudget::Graphics;
        assert_eq!(
            available_for(Graphics, 1024 * MIB, Some(1200 * MIB)),
            592 * MIB
        );
        assert_eq!(available_for(Graphics, 1024 * MIB, Some(1792 * MIB)), 0);
        assert_eq!(available_for(Graphics, 1024 * MIB, None), 64 * MIB);
    }

    #[test]
    fn measured_commit_preserves_emergency_space_without_enlarging_the_job() {
        assert_eq!(available(256 * MIB, Some(600 * MIB)), 256 * MIB);
        assert_eq!(available(256 * MIB, Some(800 * MIB)), 96 * MIB);
        assert!(admits(100 * MIB, 96 * MIB, Some(800 * MIB)));
        assert!(!admits(100 * MIB, 96 * MIB + 1, Some(800 * MIB)));
        assert_eq!(available(usize::MAX, Some(896 * MIB)), 0);
        assert!(!admits(100 * MIB, 1, Some(896 * MIB)));
        assert_eq!(RENDERER_MEMORY_LIMIT_BYTES, 1024 * MIB);
    }
    #[test]
    fn failed_observations_are_not_free_headroom() {
        assert_eq!(available(256 * MIB, None), 64 * MIB);
        assert!(admits(32 * MIB, 32 * MIB, None));
        assert!(!admits(32 * MIB, 32 * MIB + 1, None));
        assert!(!admits(64 * MIB, 1, None));
        assert_eq!(available(16 * MIB, None), 16 * MIB);
    }
    #[test]
    fn committed_reuse_and_retirement_do_not_require_new_headroom() {
        for private in [
            None,
            Some(0),
            Some(RENDERER_MEMORY_LIMIT_BYTES),
            Some(usize::MAX),
        ] {
            assert!(admits(usize::MAX, 0, private));
        }
    }
    #[test]
    fn extreme_charge_and_commit_values_fail_without_overflow() {
        assert!(!admits(usize::MAX, 1, None));
        assert!(!admits(0, usize::MAX, Some(0)));
        assert_eq!(available(usize::MAX, Some(usize::MAX)), 0);
        assert!(!admits(0, 1, Some(usize::MAX)));
    }
}
