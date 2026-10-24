//! The lowest Breeze layer: process-wide limits and budgets, process/thread
//! measurements, native GPU presentation primitives, and pixel byte arithmetic.
//! It depends on no other Breeze crate, so engine crates can share these
//! contracts without depending on the browser library. See docs/crate-layout.md.
#[cfg(windows)]
pub mod gpu_composition;
pub mod gpu_frame;
pub mod limits;
pub mod owner_cpu;
pub mod pixels;
pub mod process_memory;
pub mod renderer_budget;
// Test-process guard shared by dependent crates' native GPU tests. Production
// code never calls it; it must not be cfg(test) because dependents' test
// builds compile this crate without that cfg.
#[doc(hidden)]
pub mod test_resources;
