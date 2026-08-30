//! Restricted Windows media-worker lifecycle and capability probing.

mod backend;
mod broker;
mod child;
mod client;
pub(crate) mod launcher;

pub use broker::{
    DecodedMediaFrame, MediaSession, MediaWorkerSnapshot, MediaWorkerState, OwnedMediaDecode,
    OwnedMediaPlayback,
};
pub(crate) use client::{MediaClient, MediaClientEndpoints, RendererMediaDecode};
pub use launcher::{MediaLaunchOptions, MediaStartupFault};

/// Runs an internal media child mode before the interactive browser initializes.
pub fn run_child_from_args(arguments: &[String]) -> Option<Result<(), String>> {
    if arguments
        .iter()
        .any(|argument| argument == "--media-child-probe")
    {
        return Some(Ok(()));
    }
    arguments
        .iter()
        .any(|argument| argument == "--media-process")
        .then(|| child::run(arguments))
}
