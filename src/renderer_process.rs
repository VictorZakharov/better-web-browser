//! Windows renderer-process lifecycle and containment.

mod broker;
mod child;
mod launcher;
mod windows;

pub use broker::{
    RendererCrashSurface, RendererEvent, RendererExit, RendererExitReason, RendererSession,
    RendererSnapshot, RendererState,
};
pub use launcher::{RendererLaunchOptions, StartupFault};

/// Runs an internal child mode when the command line selects one.
///
/// Renderer mode is handled before the Win32 browser shell initializes, so automated children
/// never register or create a window. The child-probe mode exists solely to prove that the child
/// process mitigation rejects renderer descendants.
pub fn run_child_from_args(arguments: &[String]) -> Option<Result<(), String>> {
    if arguments
        .iter()
        .any(|argument| argument == "--renderer-child-probe")
    {
        return Some(Ok(()));
    }
    arguments
        .iter()
        .any(|argument| argument == "--renderer-process")
        .then(|| child::run(arguments))
}
