//! Public script inputs/outcomes and shared execution limits.

use super::network::ScriptFetchAction;
use super::workers::ScriptWorkerAction;
use crate::engine::dom::NodeRef;
use crate::engine::invalidation::RenderInvalidation;
use crate::fetch::{CredentialsMode, ReferrerPolicy, RequestMode};
use crate::storage::StorageMutation;
use std::time::Duration;

// Lifecycle dispatch no longer runs timers reentrantly. Six 250 ms slices retain the former
// effective 1.5 second virtual startup horizon while making the bound explicit.
pub(super) const STARTUP_TIMER_PASSES: usize = 6;
pub(super) const STARTUP_TIMER_SLICE: Duration = Duration::from_millis(250);

pub type DynamicScriptLoader<'a> =
    dyn FnMut(&str, ScriptKind, ScriptFetchOptions) -> Result<String, String> + 'a;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DynamicScriptRequest {
    pub source_url: String,
    pub kind: ScriptKind,
    pub fetch_options: ScriptFetchOptions,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ScriptKind {
    Classic,
    Module,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ScriptFetchOptions {
    pub mode: RequestMode,
    pub credentials: CredentialsMode,
    pub referrer_policy: ReferrerPolicy,
}

impl ScriptFetchOptions {
    // Keep element policy separate from generic Fetch defaults. Classic and module scripts use
    // different request modes and credentials rules:
    // https://html.spec.whatwg.org/multipage/scripting.html#fetch-a-classic-script
    pub fn for_element(
        kind: ScriptKind,
        cross_origin: Option<&str>,
        referrer_policy: Option<&str>,
    ) -> Self {
        let includes_credentials = cross_origin
            .is_some_and(|value| value.eq_ignore_ascii_case("use-credentials"))
            || kind == ScriptKind::Classic && cross_origin.is_none();
        let credentials = if includes_credentials {
            CredentialsMode::Include
        } else {
            CredentialsMode::SameOrigin
        };
        let mode = if kind == ScriptKind::Module || cross_origin.is_some() {
            RequestMode::Cors
        } else {
            RequestMode::NoCors
        };
        Self {
            mode,
            credentials,
            referrer_policy: parse_referrer_policy(referrer_policy),
        }
    }

    pub fn for_kind(kind: ScriptKind) -> Self {
        Self::for_element(kind, None, None)
    }
}

fn parse_referrer_policy(value: Option<&str>) -> ReferrerPolicy {
    match value
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase()
        .as_str()
    {
        "no-referrer" => ReferrerPolicy::NoReferrer,
        "no-referrer-when-downgrade" => ReferrerPolicy::NoReferrerWhenDowngrade,
        "same-origin" => ReferrerPolicy::SameOrigin,
        "origin" => ReferrerPolicy::Origin,
        "strict-origin" => ReferrerPolicy::StrictOrigin,
        "origin-when-cross-origin" => ReferrerPolicy::OriginWhenCrossOrigin,
        "unsafe-url" => ReferrerPolicy::UnsafeUrl,
        _ => ReferrerPolicy::StrictOriginWhenCrossOrigin,
    }
}

#[derive(Debug, Clone)]
pub struct ScriptInput {
    pub node: NodeRef,
    pub source_url: String,
    pub code: String,
    pub kind: ScriptKind,
    pub fetch_options: ScriptFetchOptions,
    pub finish_lifecycle: bool,
}

#[derive(Debug, Default, Clone)]
pub struct ScriptOutcome {
    pub executed: usize,
    pub mutation_count: usize,
    pub errors: Vec<String>,
    pub console: Vec<String>,
    pub diagnostics: Vec<String>,
    pub navigation_url: Option<String>,
    pub cookie_updates: Vec<String>,
    pub storage_updates: Vec<StorageMutation>,
    pub fetch_actions: Vec<ScriptFetchAction>,
    pub worker_actions: Vec<ScriptWorkerAction>,
    pub fullscreen_actions: Vec<ScriptFullscreenAction>,
    pub media_actions: Vec<ScriptMediaAction>,
    pub runtime_stopped: bool,
    pub render_requested: bool,
    pub invalidation: RenderInvalidation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScriptFullscreenAction {
    pub request_id: u64,
    pub enter: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScriptMediaAction {
    pub request_id: u64,
    pub node: crate::engine::dom::NodeId,
    pub play: bool,
    pub volume_millis: u16,
    pub muted: bool,
}

impl ScriptOutcome {
    pub(super) fn record_timing(&mut self, label: &str, elapsed: Duration) {
        if elapsed.as_millis() >= 1 {
            self.diagnostics
                .push(format!("{label} {:.3} ms", elapsed.as_secs_f64() * 1_000.0));
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct UserInputModifiers {
    pub alt: bool,
    pub control: bool,
    pub shift: bool,
    pub meta: bool,
}

#[derive(Debug, Clone)]
pub enum UserInputEvent {
    Pointer {
        target: Option<NodeRef>,
        phase: &'static str,
        button: u8,
        buttons: u8,
        x: f32,
        y: f32,
        activate: bool,
        modifiers: UserInputModifiers,
    },
    Keyboard {
        target: Option<NodeRef>,
        phase: &'static str,
        key: String,
        code: String,
        key_code: u32,
        repeat: bool,
        modifiers: UserInputModifiers,
    },
    Text {
        target: NodeRef,
        value: String,
        selection_start: u32,
        selection_end: u32,
    },
    Focus {
        target: Option<NodeRef>,
        focused: bool,
    },
    Simple {
        target: NodeRef,
        event_type: &'static str,
        bubbles: bool,
        cancelable: bool,
    },
    Scroll {
        x: f32,
        y: f32,
    },
    Viewport {
        width: f32,
        height: f32,
        scale: f32,
    },
    Lifecycle {
        state: &'static str,
        previous: &'static str,
    },
    Fullscreen {
        request_id: u64,
        disposition: &'static str,
    },
    Media {
        target: NodeRef,
        request_id: u64,
        disposition: &'static str,
        current_time: f64,
        duration: f64,
        width: u32,
        height: u32,
    },
}

#[derive(Debug, Default, Clone)]
pub struct UserInputResult {
    pub outcome: ScriptOutcome,
    pub default_allowed: bool,
}

pub(crate) fn is_classic_javascript_type(script_type: &str) -> bool {
    matches!(
        script_type.trim().to_ascii_lowercase().as_str(),
        "" | "text/javascript"
            | "application/javascript"
            | "text/ecmascript"
            | "application/ecmascript"
    )
}
