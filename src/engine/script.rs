use super::css::{StyleSet, resolved_property_value};
use super::dom::{Node, NodeData, NodeId, NodeRef};
use super::scheduler::{EventLoopScheduler, ScheduledWork, TaskHandle, TaskSource};
use crate::navigation::resolve_url;
use boa_engine::{
    Context, Finalize, JsData, JsNativeError, JsResult, JsString, JsValue, NativeFunction, Source,
    Trace, property::Attribute,
};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::{Rc, Weak};
use std::time::{Duration, Instant};

mod binding_helpers;
mod bootstrap;
mod dom_host;
mod execution;
mod host_call;
mod host_state;
mod mutation_host;
mod runtime;
mod style_host;

pub use execution::{execute, execute_with_loader};
use host_call::host_call;
use host_state::{HostState, HostStateLink};
pub use runtime::ScriptRuntime;

const MAX_LOOP_ITERATIONS: u64 = if cfg!(test) { 25_000 } else { 5_000_000 };
const MAX_SCRIPT_BYTES: usize = 8 * 1024 * 1024;
const MAX_DYNAMIC_SCRIPTS: usize = 32;
// This makes the former effective startup horizon explicit. The JavaScript shim used to advance
// 200 ms while dispatching lifecycle events plus five 250 ms settlement slices. Lifecycle dispatch
// no longer runs timer tasks reentrantly, so use six slices for a clear 1.5 second virtual budget.
const STARTUP_TIMER_PASSES: usize = 6;
const STARTUP_TIMER_SLICE: Duration = Duration::from_millis(250);
const MAX_TIMER_CALLBACKS_PER_SLICE: usize = 128;

pub type DynamicScriptLoader<'a> = dyn FnMut(&str) -> Result<String, String> + 'a;

#[derive(Debug, Clone)]
pub struct ScriptInput {
    pub node: NodeRef,
    pub source_url: String,
    pub code: String,
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
    pub runtime_stopped: bool,
    pub render_requested: bool,
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

#[cfg(test)]
#[path = "script/tests/mod.rs"]
mod tests;
