use super::css::{StyleSet, resolved_property_value};
use super::dom::{Node, NodeData, NodeId, NodeRef};
use super::invalidation::{InvalidationImpact, MutationKind, RenderInvalidation};
use super::scheduler::{EventLoopScheduler, ScheduledWork, TaskHandle, TaskSource};
use crate::limits::{
    MAX_DOM_NODES, MAX_DYNAMIC_SCRIPTS,
    MAX_POST_LOAD_TIMER_CALLBACKS as MAX_TIMER_CALLBACKS_PER_SLICE, MAX_SCRIPT_BYTES,
    MAX_SCRIPT_LOOP_ITERATIONS as MAX_LOOP_ITERATIONS,
};
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
mod dynamic_scripts;
mod execution;
mod host_call;
mod host_profiling;
mod host_state;
mod module_evaluation;
mod module_lifecycle;
mod module_loader;
mod mutation_host;
mod network;
mod render_invalidation;
mod runtime;
mod runtime_guard;
mod style_cache;
mod style_host;
mod types;
mod user_events;
mod worker_bootstrap;
mod worker_host;
mod worker_module;
mod worker_runtime;
mod workers;

pub use execution::{execute, execute_with_loader};
use host_call::host_call;
use host_state::{HostState, HostStateLink};
pub use network::ScriptFetchAction;
pub use runtime::ScriptRuntime;
pub(crate) use types::is_classic_javascript_type;
pub use types::{
    DynamicScriptLoader, ScriptInput, ScriptKind, ScriptOutcome, UserInputEvent,
    UserInputModifiers, UserInputResult,
};
use types::{STARTUP_TIMER_PASSES, STARTUP_TIMER_SLICE};
pub use worker_host::WorkerSourceLoader;
pub use worker_runtime::{WorkerRuntime, WorkerRuntimeOutcome};
pub use workers::ScriptWorkerAction;

#[cfg(test)]
#[path = "script/tests/mod.rs"]
mod tests;
