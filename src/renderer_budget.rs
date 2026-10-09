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
            Self::Graphics => "Renderer: 2 GiB (3D workloads)",
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
            Self::Graphics => 2 * 1024 * 1024 * 1024,
        }
    }

    pub(crate) const fn gpu_context_bytes(self) -> usize {
        match self {
            Self::Standard => 256 * 1024 * 1024,
            Self::Graphics => 1024 * 1024 * 1024,
        }
    }

    pub(crate) const fn gpu_owner_bytes(self) -> usize {
        match self {
            Self::Standard => 512 * 1024 * 1024,
            Self::Graphics => 1024 * 1024 * 1024,
        }
    }
}

static RENDERER_BUDGET: OnceLock<RendererBudget> = OnceLock::new();

// Only isolated-child bootstrap installs this before any realm/GPU owner starts.
// The broker uses the same typed option for its kernel-enforced Job limit.
pub(crate) fn install(budget: RendererBudget) -> Result<(), &'static str> {
    RENDERER_BUDGET
        .set(budget)
        .map_err(|_| "renderer memory budget was already initialized")
}

pub(crate) fn current() -> RendererBudget {
    RENDERER_BUDGET.get().copied().unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn absent_budget_is_standard_and_only_two_finite_modes_exist() {
        assert_eq!(RendererBudget::default(), RendererBudget::Standard);
        for mode in [RendererBudget::Standard, RendererBudget::Graphics] {
            assert_eq!(RendererBudget::parse(mode.setting()), Some(mode));
            assert!(mode.gpu_context_bytes() <= mode.gpu_owner_bytes());
            assert!(mode.gpu_owner_bytes() < mode.bytes());
        }
        for invalid in ["", "0", "unlimited", "4294967296", "Graphics", "graphics\0"] {
            assert_eq!(RendererBudget::parse(invalid), None);
        }
        assert_eq!(RendererBudget::Standard.bytes(), 1024 * 1024 * 1024);
        assert_eq!(RendererBudget::Graphics.bytes(), 2 * 1024 * 1024 * 1024);
    }
}
