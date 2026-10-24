//! Private commit is an extra admission signal, not a replacement for GPU
//! accounting or the broker's hard Job. Leave room for JS, compiler and IPC work.
const UNKNOWN_SAMPLE_CEILING: usize = 64 * 1024 * 1024;
const MAX_EMERGENCY_RESERVE: usize = 128 * 1024 * 1024;

pub(super) fn available(ceiling: usize, private: Option<usize>) -> usize {
    available_for(
        breeze_platform::renderer_budget::current(),
        ceiling,
        private,
    )
}

fn available_for(
    budget: breeze_platform::renderer_budget::RendererBudget,
    ceiling: usize,
    private: Option<usize>,
) -> usize {
    let limit = budget.bytes();
    // Reserve bounded space for JS/compiler/IPC work. Scaling this reserve
    // with the opt-in graphics Job needlessly doubled it to 256 MiB: HiDPI
    // render-target replacement then failed below the unchanged hard limit.
    let reserve = (limit / 8).min(MAX_EMERGENCY_RESERVE);
    match private {
        Some(private) => ceiling.min(limit.saturating_sub(reserve).saturating_sub(private)),
        None => ceiling.min(UNKNOWN_SAMPLE_CEILING),
    }
}

pub(super) fn admits(charged: usize, growth: usize, private: Option<usize>) -> bool {
    admits_for(
        breeze_platform::renderer_budget::current(),
        charged,
        growth,
        private,
    )
}

fn admits_for(
    budget: breeze_platform::renderer_budget::RendererBudget,
    charged: usize,
    growth: usize,
    private: Option<usize>,
) -> bool {
    if growth == 0 {
        return true;
    }
    match private {
        Some(_) => growth <= available_for(budget, usize::MAX, private),
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
    use breeze_platform::limits::RENDERER_MEMORY_LIMIT_BYTES;
    const MIB: usize = 1024 * 1024;

    #[test]
    fn larger_graphics_ledger_still_rejects_growth_without_private_headroom() {
        use breeze_platform::renderer_budget::RendererBudget::Graphics;
        let charged = 1100 * MIB;
        let growth = 32 * MIB;
        assert!(charged + growth < Graphics.gpu_context_bytes());
        assert!(admits_for(Graphics, charged, growth, Some(2912 * MIB)));
        assert!(!admits_for(Graphics, charged, growth + 1, Some(2912 * MIB)));
        assert!(!admits_for(Graphics, charged, 1, Some(2944 * MIB)));
        assert!(!admits_for(Graphics, charged, growth, None));
        assert!(admits_for(Graphics, charged, 0, None));
    }

    #[test]
    fn graphics_mode_retains_the_standard_emergency_reserve_inside_its_job() {
        use breeze_platform::renderer_budget::RendererBudget::Graphics;
        assert_eq!(
            available_for(Graphics, 3 * 1024 * MIB, Some(1200 * MIB)),
            1744 * MIB
        );
        assert_eq!(available_for(Graphics, 1024 * MIB, Some(2944 * MIB)), 0);
        assert_eq!(available_for(Graphics, 1024 * MIB, None), 64 * MIB);
    }

    #[test]
    fn graphics_resize_can_replace_storage_without_spending_emergency_space() {
        use breeze_platform::renderer_budget::RendererBudget::Graphics;
        // A measured failing fullscreen allocation: the old fractional reserve
        // refused 16 MiB with almost 268 MiB still below the 2 GiB Job limit.
        let private = 1_867_014_144;
        let growth = 16_588_800;
        assert!(admits_for(Graphics, 1_184_073_496, growth, Some(private)));
        assert!(Graphics.bytes() - private - growth >= MAX_EMERGENCY_RESERVE);
        // Even the capped reserve still refused later target replacements in
        // the old 2 GiB process. The opt-in 3 GiB Job covers that measured peak.
        let peak = 2_092_449_792;
        assert!(admits_for(Graphics, 1_260_077_464, growth, Some(peak)));
        assert!(Graphics.bytes() - peak - growth >= MAX_EMERGENCY_RESERVE);
        assert!(!admits_for(Graphics, 0, 1, Some(Graphics.bytes())));
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
