//! Browser-library limits. Shared limits live in `breeze_platform::limits`.
//! The two below are smaller in this crate's unit tests so runaway-script cases
//! stay fast; they live here because `cfg(test)` is set only for the crate under
//! test, never for its dependencies.
pub use breeze_platform::limits::*;

pub const MAX_DOM_TREE_MUTATIONS_PER_TASK: usize = if cfg!(test) { 256 } else { 10_000 };
pub const MAX_SCRIPT_LOOP_ITERATIONS: u64 = if cfg!(test) { 25_000 } else { 5_000_000 };
