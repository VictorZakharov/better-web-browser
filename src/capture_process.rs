//! Private, browser-controlled device capture process.
//!
//! No renderer handle or JavaScript binding is connected to this module. The process and its
//! hardware capabilities are created only after a future browser permission service admits a
//! concrete camera/microphone grant. Tests use a deterministic provider and never open devices.

mod broker;
mod child;
pub(crate) mod launcher;
mod native;

#[allow(unused_imports)]
// Browser permission wiring is intentionally out of this foundation PR.
pub(crate) use broker::CaptureSession;

/// Internal child role, dispatched before any interactive browser initialization.
pub fn run_child_from_args(arguments: &[String]) -> Option<Result<(), String>> {
    arguments
        .iter()
        .any(|argument| argument == "--capture-process")
        .then(|| child::run(arguments))
}
