//! Browser-owned, finite renderer budgets. Pages cannot select or enlarge them.
use std::sync::OnceLock;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RendererBudget {
    #[default]
    Standard,
    Graphics,
}

impl RendererBudget {
    pub const fn setting(self) -> &'static str {
        match self {
            Self::Standard => "standard",
            Self::Graphics => "graphics",
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Standard => "Renderer: 1 GiB (default)",
            Self::Graphics => "Renderer: 3 GiB (3D workloads)",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "standard" => Some(Self::Standard),
            "graphics" => Some(Self::Graphics),
            _ => None,
        }
    }

    pub const fn bytes(self) -> usize {
        match self {
            Self::Standard => crate::limits::RENDERER_MEMORY_LIMIT_BYTES,
            // This process also owns ANGLE and its transient target storage.
            // A measured High-quality fullscreen workload exceeded 2 GiB's
            // soft headroom despite fitting the independent GPU ledger.
            Self::Graphics => 3 * 1024 * 1024 * 1024,
        }
    }

    pub const fn gpu_context_bytes(self) -> usize {
        match self {
            Self::Standard => 256 * 1024 * 1024,
            // Storage reservations count two copies and lifetime high-water
            // capacities. Permit HiDPI render targets without enlarging the
            // selected Job or bypassing the independent private-headroom guard.
            Self::Graphics => 1280 * 1024 * 1024,
        }
    }

    pub const fn gpu_owner_bytes(self) -> usize {
        match self {
            Self::Standard => 512 * 1024 * 1024,
            Self::Graphics => self.gpu_context_bytes(),
        }
    }

    /// Native page bitmaps live in the browser process, outside the renderer Job.
    /// The graphics choice admits the larger decoded document plus its HUD rasters.
    pub const fn presentation_bitmap_bytes(self) -> usize {
        match self {
            Self::Standard => 64 * 1024 * 1024,
            Self::Graphics => 128 * 1024 * 1024,
        }
    }

    /// Decoded DOM, CSS, SVG and Canvas snapshots share this document allowance.
    /// HiDPI graphics surfaces can coexist during an author-created transition;
    /// the opt-in graphics profile must budget those pixels as well as GPU data.
    pub const fn decoded_image_bytes(self) -> usize {
        match self {
            Self::Standard => crate::limits::MAX_PAGE_DECODED_IMAGE_BYTES,
            Self::Graphics => 128 * 1024 * 1024,
        }
    }
}

static RENDERER_BUDGET: OnceLock<RendererBudget> = OnceLock::new();

// Only isolated-child bootstrap installs this before any realm/GPU owner starts.
// The broker uses the same typed option for its kernel-enforced Job limit.
pub fn install(budget: RendererBudget) -> Result<(), &'static str> {
    RENDERER_BUDGET
        .set(budget)
        .map_err(|_| "renderer memory budget was already initialized")
}

pub fn current() -> RendererBudget {
    RENDERER_BUDGET.get().copied().unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_presentation_has_finite_separate_pixel_limits() {
        assert_eq!(
            RendererBudget::Standard.presentation_bitmap_bytes(),
            64 * 1024 * 1024
        );
        assert_eq!(
            RendererBudget::Graphics.presentation_bitmap_bytes(),
            128 * 1024 * 1024
        );
        // This cache is outside the child Job; changing it cannot enlarge that Job.
        assert_eq!(RendererBudget::Standard.bytes(), 1024 * 1024 * 1024);
        assert_eq!(RendererBudget::Graphics.bytes(), 3 * 1024 * 1024 * 1024);
    }

    #[test]
    fn absent_budget_is_standard_and_only_two_finite_modes_exist() {
        assert_eq!(RendererBudget::default(), RendererBudget::Standard);
        for mode in [RendererBudget::Standard, RendererBudget::Graphics] {
            assert_eq!(RendererBudget::parse(mode.setting()), Some(mode));
            assert!(mode.gpu_context_bytes() <= mode.gpu_owner_bytes());
            assert!(mode.gpu_owner_bytes() < mode.bytes());
            assert!(mode.decoded_image_bytes() < mode.bytes());
        }
        for invalid in ["", "0", "unlimited", "4294967296", "Graphics", "graphics\0"] {
            assert_eq!(RendererBudget::parse(invalid), None);
        }
        assert_eq!(RendererBudget::Standard.bytes(), 1024 * 1024 * 1024);
        assert_eq!(RendererBudget::Graphics.bytes(), 3 * 1024 * 1024 * 1024);
        assert_eq!(
            RendererBudget::Standard.decoded_image_bytes(),
            64 * 1024 * 1024
        );
        assert_eq!(
            RendererBudget::Graphics.decoded_image_bytes(),
            128 * 1024 * 1024
        );
    }
}
