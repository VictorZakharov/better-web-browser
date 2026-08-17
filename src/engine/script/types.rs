//! Public script inputs/outcomes and shared execution limits.

use super::network::ScriptFetchAction;
use super::workers::ScriptWorkerAction;
use crate::engine::dom::NodeRef;
use crate::engine::invalidation::RenderInvalidation;
use std::time::Duration;

// Lifecycle dispatch no longer runs timers reentrantly. Six 250 ms slices retain the former
// effective 1.5 second virtual startup horizon while making the bound explicit.
pub(super) const STARTUP_TIMER_PASSES: usize = 6;
pub(super) const STARTUP_TIMER_SLICE: Duration = Duration::from_millis(250);

pub type DynamicScriptLoader<'a> = dyn FnMut(&str, ScriptKind) -> Result<String, String> + 'a;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ScriptKind {
    Classic,
    Module,
}

#[derive(Debug, Clone)]
pub struct ScriptInput {
    pub node: NodeRef,
    pub source_url: String,
    pub code: String,
    pub kind: ScriptKind,
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
    pub fetch_actions: Vec<ScriptFetchAction>,
    pub worker_actions: Vec<ScriptWorkerAction>,
    pub runtime_stopped: bool,
    pub render_requested: bool,
    pub invalidation: RenderInvalidation,
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
