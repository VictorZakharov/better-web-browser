//! Unit tests of this crate and of its dependents use the deterministic WARP
//! provider. `cfg(test)` is set only for the crate under test, so a dependent's
//! test build opts in at run time before creating its first context.
use std::sync::atomic::{AtomicBool, Ordering};

static SELECTED: AtomicBool = AtomicBool::new(false);

/// Select the software provider for every later context in this process.
/// Test-only for dependent crates; production never calls it.
#[doc(hidden)]
pub fn select() {
    SELECTED.store(true, Ordering::Relaxed);
}

pub(crate) fn selected() -> bool {
    cfg!(test) || SELECTED.load(Ordering::Relaxed)
}
