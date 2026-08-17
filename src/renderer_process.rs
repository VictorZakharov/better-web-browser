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

const RENDERER_ENVIRONMENT_ALLOWLIST: &[&str] = &[
    "ALLUSERSPROFILE",
    "APPDATA",
    "CommonProgramFiles",
    "CommonProgramFiles(x86)",
    "CommonProgramW6432",
    "ComSpec",
    "HOMEDRIVE",
    "HOMEPATH",
    "LOCALAPPDATA",
    "NUMBER_OF_PROCESSORS",
    "OS",
    "Path",
    "PATHEXT",
    "PROCESSOR_ARCHITECTURE",
    "ProgramData",
    "ProgramFiles",
    "ProgramFiles(x86)",
    "ProgramW6432",
    "PUBLIC",
    "SystemDrive",
    "SystemRoot",
    "TEMP",
    "TMP",
    "USERNAME",
    "USERPROFILE",
    "WINDIR",
];

fn renderer_environment_name_allowed(name: &str) -> bool {
    RENDERER_ENVIRONMENT_ALLOWLIST
        .iter()
        .any(|allowed| name.eq_ignore_ascii_case(allowed))
        || {
            let bytes = name.as_bytes();
            bytes.len() == 3
                && bytes[0] == b'='
                && bytes[1].is_ascii_alphabetic()
                && bytes[2] == b':'
        }
}

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
