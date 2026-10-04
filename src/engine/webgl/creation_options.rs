//! Closed context-creation IPC attributes shared by storage and native creation.
use super::ApiVersion;

#[derive(Clone, Copy, serde::Deserialize)]
#[serde(default, deny_unknown_fields)]
pub(super) struct Options {
    pub api: ApiVersion,
    pub alpha: bool,
    pub depth: bool,
    pub stencil: bool,
    pub antialias: bool,
    pub preserve: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            api: ApiVersion::One,
            alpha: true,
            depth: true,
            stencil: false,
            antialias: false,
            preserve: false,
        }
    }
}
