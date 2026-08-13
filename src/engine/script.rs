use super::css::{is_hidden_by_html_rendering, user_agent_style_property};
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

mod runtime;

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

#[derive(Debug, Clone)]
struct PendingDynamicScript {
    node: NodeRef,
    source_url: String,
}

struct HostState {
    document: NodeRef,
    document_url: String,
    nodes: HashMap<u32, NodeRef>,
    node_ids: HashMap<NodeId, u32>,
    next_node_id: u32,
    mutation_count: usize,
    console: Vec<String>,
    navigation_url: Option<String>,
    cookies: HashMap<String, String>,
    cookie_updates: Vec<String>,
    executed: usize,
    diagnostics: Vec<String>,
    pending_dynamic_scripts: Vec<PendingDynamicScript>,
    started_dynamic_scripts: HashSet<NodeId>,
    timers: EventLoopScheduler<u32>,
    timer_handles: HashMap<u32, TaskHandle>,
}

/// A Boa context owns only a weak link to native document state. If an evaluator panic requires
/// leaking the damaged context, the page DOM and scheduler can still be released normally.
#[derive(Clone, Finalize, JsData, Trace)]
#[boa_gc(unsafe_empty_trace)]
struct HostStateLink(Weak<RefCell<HostState>>);

impl HostState {
    fn new(document: NodeRef, document_url: &str) -> Self {
        let mut state = Self {
            document,
            document_url: document_url.to_string(),
            nodes: HashMap::new(),
            node_ids: HashMap::new(),
            next_node_id: 1,
            mutation_count: 0,
            console: Vec::new(),
            navigation_url: None,
            cookies: HashMap::new(),
            cookie_updates: Vec::new(),
            executed: 0,
            diagnostics: Vec::new(),
            pending_dynamic_scripts: Vec::new(),
            started_dynamic_scripts: HashSet::new(),
            timers: EventLoopScheduler::new(),
            timer_handles: HashMap::new(),
        };
        let document = state.document.clone();
        state.register_subtree(&document);
        state
    }

    fn id_for(&mut self, node: &NodeRef) -> u32 {
        let node_id = node.id();
        if let Some(id) = self.node_ids.get(&node_id) {
            return *id;
        }
        let id = self.next_node_id;
        self.next_node_id = self.next_node_id.saturating_add(1);
        self.node_ids.insert(node_id, id);
        self.nodes.insert(id, node.clone());
        id
    }

    fn node(&self, id: u32) -> Option<NodeRef> {
        self.nodes.get(&id).cloned()
    }

    fn register_subtree(&mut self, root: &NodeRef) {
        let mut stack = vec![root.clone()];
        while let Some(node) = stack.pop() {
            self.id_for(&node);
            stack.extend(node.children.borrow().iter().rev().cloned());
            if let Some(contents) = node
                .element()
                .and_then(|element| element.template_contents.borrow().clone())
            {
                stack.push(contents);
            }
        }
    }

    fn resolved_url(&self, reference: &str) -> String {
        resolve_url(&self.document_url, reference).unwrap_or_else(|| reference.to_string())
    }

    fn diagnose(&mut self, message: String) {
        if self.diagnostics.len() < 64 {
            self.diagnostics.push(message);
        }
    }

    fn record_mutation(&mut self) {
        self.mutation_count += 1;
        self.timers.request_render();
    }

    fn schedule_timer(&mut self, id: u32, delay: Duration, repeat: bool) {
        if let Some(previous) = self.timer_handles.remove(&id) {
            self.timers.cancel(previous);
        }
        let handle = if repeat {
            self.timers.queue_repeating_task(
                TaskSource::Timer,
                delay,
                delay.max(Duration::from_millis(1)),
                id,
            )
        } else {
            self.timers.queue_task(TaskSource::Timer, delay, id)
        };
        self.timer_handles.insert(id, handle);
    }

    fn cancel_timer(&mut self, id: u32) -> bool {
        self.timer_handles
            .remove(&id)
            .is_some_and(|handle| self.timers.cancel(handle))
    }

    fn take_ready_timer(&mut self) -> Option<u32> {
        let mut ready = None;
        self.timers.run_one_task(|_, work| {
            if let ScheduledWork::Task(task) = work {
                ready = Some((task.payload, task.repeating));
            }
        });
        let (id, repeating) = ready?;
        if !repeating {
            self.timer_handles.remove(&id);
        }
        Some(id)
    }

    fn timer_summary(&self) -> String {
        let now = self.timers.now();
        let mut timers = self
            .timer_handles
            .iter()
            .filter_map(|(id, handle)| {
                self.timers.scheduled_for(*handle).map(|due| {
                    (
                        due,
                        *id,
                        format!("{id}@{}", due.saturating_sub(now).as_millis()),
                    )
                })
            })
            .collect::<Vec<_>>();
        timers.sort_by_key(|(due, id, _)| (*due, *id));
        timers
            .into_iter()
            .map(|(_, _, summary)| summary)
            .collect::<Vec<_>>()
            .join(",")
    }

    fn is_connected(&self, node: &NodeRef) -> bool {
        let mut current = Some(node.clone());
        while let Some(node) = current {
            if node.id() == self.document.id() {
                return true;
            }
            current = node.parent();
        }
        false
    }

    fn queue_dynamic_script(&mut self, node: &NodeRef) {
        if node.tag_name() != Some("script") || !self.is_connected(node) {
            return;
        }
        let script_type = node.attr("type").unwrap_or_default();
        if !is_classic_javascript_type(&script_type) {
            return;
        }
        let Some(source) = node.attr("src").filter(|source| !source.trim().is_empty()) else {
            return;
        };
        if !self.started_dynamic_scripts.insert(node.id()) {
            return;
        }
        self.pending_dynamic_scripts.push(PendingDynamicScript {
            node: node.clone(),
            source_url: self.resolved_url(source.trim()),
        });
        self.diagnose("queued dynamically inserted external script".into());
    }

    fn cookie_header(&self) -> String {
        let mut cookies = self.cookies.iter().collect::<Vec<_>>();
        cookies.sort_unstable_by(|left, right| left.0.cmp(right.0));
        cookies
            .into_iter()
            .map(|(name, value)| format!("{name}={value}"))
            .collect::<Vec<_>>()
            .join("; ")
    }

    fn set_cookie(&mut self, assignment: String) {
        let Some(pair) = assignment.split(';').next().map(str::trim) else {
            return;
        };
        let Some((name, value)) = pair.split_once('=') else {
            return;
        };
        let name = name.trim();
        if name.is_empty() || name.bytes().any(|byte| byte <= 0x20 || byte == b';') {
            return;
        }

        let expired = assignment.split(';').skip(1).any(|attribute| {
            attribute
                .trim()
                .split_once('=')
                .is_some_and(|(name, value)| {
                    name.trim().eq_ignore_ascii_case("max-age")
                        && value
                            .trim()
                            .parse::<i64>()
                            .is_ok_and(|seconds| seconds <= 0)
                })
        });
        if expired {
            self.cookies.remove(name);
        } else {
            self.cookies
                .insert(name.to_string(), value.trim().to_string());
        }
        self.cookie_updates.push(assignment);
    }
}

pub fn execute(document: NodeRef, document_url: &str, scripts: &[ScriptInput]) -> ScriptOutcome {
    execute_impl(document, document_url, scripts, None)
}

pub fn execute_with_loader(
    document: NodeRef,
    document_url: &str,
    scripts: &[ScriptInput],
    dynamic_script_loader: &mut DynamicScriptLoader<'_>,
) -> ScriptOutcome {
    execute_impl(document, document_url, scripts, Some(dynamic_script_loader))
}

fn execute_impl(
    document: NodeRef,
    document_url: &str,
    scripts: &[ScriptInput],
    dynamic_script_loader: Option<&mut DynamicScriptLoader<'_>>,
) -> ScriptOutcome {
    if scripts.is_empty() {
        return ScriptOutcome::default();
    }

    ScriptRuntime::new(document, document_url)
        .execute_initial_with_loader(scripts, dynamic_script_loader)
}

fn panic_detail(payload: Box<dyn std::any::Any + Send>) -> String {
    payload
        .downcast_ref::<&str>()
        .map(|message| (*message).to_string())
        .or_else(|| payload.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "unknown evaluator panic".to_string())
}

fn stopped_runtime_outcome(detail: String) -> ScriptOutcome {
    ScriptOutcome {
        errors: vec![format!(
            "JavaScript runtime was stopped safely after an evaluator failure: {detail}"
        )],
        runtime_stopped: true,
        ..ScriptOutcome::default()
    }
}

fn inactive_runtime_outcome() -> ScriptOutcome {
    ScriptOutcome {
        errors: vec!["JavaScript runtime is inactive because its document was cancelled".into()],
        runtime_stopped: true,
        ..ScriptOutcome::default()
    }
}

fn lifecycle_error(message: &str) -> ScriptOutcome {
    ScriptOutcome {
        errors: vec![format!("JavaScript runtime lifecycle: {message}")],
        ..ScriptOutcome::default()
    }
}

fn execute_inner(
    scripts: &[ScriptInput],
    context: &mut Context,
    host: &Rc<RefCell<HostState>>,
    total_bytes: &mut usize,
    dynamic_script_loader: &mut Option<&mut DynamicScriptLoader<'_>>,
) -> ScriptOutcome {
    let mut outcome = ScriptOutcome::default();
    context
        .runtime_limits_mut()
        .set_loop_iteration_limit(MAX_LOOP_ITERATIONS);
    context.runtime_limits_mut().set_recursion_limit(128);

    if let Err(error) = context.register_global_builtin_callable(
        boa_engine::js_string!("__hostCall"),
        1,
        NativeFunction::from_fn_ptr(host_call),
    ) {
        outcome
            .errors
            .push(format!("initialize JavaScript host bridge: {error}"));
        return outcome;
    }

    let iframe_realm = match context.create_realm() {
        Ok(realm) => realm,
        Err(error) => {
            outcome
                .errors
                .push(format!("initialize iframe JavaScript realm: {error}"));
            return outcome;
        }
    };
    let parent_realm = context.enter_realm(iframe_realm);
    let iframe_bootstrap = context.eval(Source::from_bytes(IFRAME_REALM_BOOTSTRAP));
    let iframe_window = context.global_object();
    context.enter_realm(parent_realm);
    if let Err(error) = iframe_bootstrap {
        outcome
            .errors
            .push(format!("initialize iframe browser bindings: {error}"));
        return outcome;
    }
    if let Err(error) = context.register_global_property(
        boa_engine::js_string!("__iframeWindow"),
        iframe_window,
        Attribute::all(),
    ) {
        outcome
            .errors
            .push(format!("expose iframe JavaScript realm: {error}"));
        return outcome;
    }

    if let Err(error) = context.eval(Source::from_bytes(BROWSER_BOOTSTRAP)) {
        outcome
            .errors
            .push(format!("initialize browser bindings: {error}"));
        return outcome;
    }

    for script in scripts {
        if total_bytes.saturating_add(script.code.len()) > MAX_SCRIPT_BYTES {
            outcome.errors.push(format!(
                "{}: skipped because the page exceeds the {} MiB JavaScript limit",
                script.source_url,
                MAX_SCRIPT_BYTES / 1024 / 1024
            ));
            break;
        }
        *total_bytes += script.code.len();

        evaluate_script(context, host, &mut outcome, script, false);
        drain_dynamic_scripts(
            context,
            host,
            &mut outcome,
            dynamic_script_loader,
            total_bytes,
        );
    }

    // An async-only document still completes parsing before its first external script arrives.
    let finish_lifecycle =
        scripts.is_empty() || scripts.iter().any(|script| script.finish_lifecycle);
    let lifecycle = if finish_lifecycle {
        "document.__setCurrentScript(0); __finishDocument();"
    } else {
        "document.__setCurrentScript(0);"
    };
    if let Err(error) = context.eval(Source::from_bytes(lifecycle)) {
        outcome
            .errors
            .push(format!("finish document lifecycle: {error}"));
    }
    if let Err(error) = context.run_jobs() {
        outcome.errors.push(format!("finish promise jobs: {error}"));
    }
    drain_dynamic_scripts(
        context,
        host,
        &mut outcome,
        dynamic_script_loader,
        total_bytes,
    );
    for _ in 0..STARTUP_TIMER_PASSES {
        settle_startup_timer_slice(
            context,
            host,
            &mut outcome,
            dynamic_script_loader,
            total_bytes,
        );
    }

    append_timer_summary(host, &mut outcome);

    outcome
}

fn execute_additional_inner(
    scripts: &[ScriptInput],
    context: &mut Context,
    host: &Rc<RefCell<HostState>>,
    total_bytes: &mut usize,
    dynamic_script_loader: &mut Option<&mut DynamicScriptLoader<'_>>,
) -> ScriptOutcome {
    let mut outcome = ScriptOutcome::default();
    for script in scripts {
        if total_bytes.saturating_add(script.code.len()) > MAX_SCRIPT_BYTES {
            outcome.errors.push(format!(
                "{}: skipped because the page exceeds the {} MiB JavaScript limit",
                script.source_url,
                MAX_SCRIPT_BYTES / 1024 / 1024
            ));
            continue;
        }
        *total_bytes += script.code.len();
        evaluate_script(context, host, &mut outcome, script, true);
        drain_dynamic_scripts(
            context,
            host,
            &mut outcome,
            dynamic_script_loader,
            total_bytes,
        );
    }

    if let Err(error) = context.eval(Source::from_bytes("document.__setCurrentScript(0);")) {
        outcome
            .errors
            .push(format!("finish additional script task: {error}"));
    }
    if let Err(error) = context.run_jobs() {
        outcome
            .errors
            .push(format!("finish additional script promise jobs: {error}"));
    }
    drain_dynamic_scripts(
        context,
        host,
        &mut outcome,
        dynamic_script_loader,
        total_bytes,
    );
    append_timer_summary(host, &mut outcome);
    outcome
}

fn append_timer_summary(host: &Rc<RefCell<HostState>>, outcome: &mut ScriptOutcome) {
    let timer_summary = host.borrow().timer_summary();
    if !timer_summary.is_empty() {
        outcome
            .diagnostics
            .push(format!("JavaScript timers after settling: {timer_summary}"));
    }
}

fn settle_startup_timer_slice(
    context: &mut Context,
    host: &Rc<RefCell<HostState>>,
    outcome: &mut ScriptOutcome,
    dynamic_script_loader: &mut Option<&mut DynamicScriptLoader<'_>>,
    total_bytes: &mut usize,
) {
    settle_timer_slice(
        context,
        host,
        outcome,
        dynamic_script_loader,
        total_bytes,
        STARTUP_TIMER_SLICE,
        MAX_TIMER_CALLBACKS_PER_SLICE,
    );
}

fn settle_timer_slice(
    context: &mut Context,
    host: &Rc<RefCell<HostState>>,
    outcome: &mut ScriptOutcome,
    dynamic_script_loader: &mut Option<&mut DynamicScriptLoader<'_>>,
    total_bytes: &mut usize,
    advance: Duration,
    max_callbacks: usize,
) {
    let horizon = host.borrow().timers.now().saturating_add(advance);

    for _ in 0..max_callbacks {
        let timer_id = {
            let mut host = host.borrow_mut();
            let due = host.timers.next_due_time();
            due.filter(|due| *due <= horizon).and_then(|due| {
                host.timers.advance_to(due);
                host.take_ready_timer()
            })
        };
        let Some(timer_id) = timer_id else {
            break;
        };

        let invocation = format!("__runTimer({timer_id});");
        if let Err(error) = context.eval(Source::from_bytes(&invocation)) {
            outcome
                .errors
                .push(format!("JavaScript timer {timer_id}: {error}"));
        }
        // HTML performs a microtask checkpoint after every task. Boa owns the Promise job queue,
        // so drain it here rather than once after a whole batch of timer callbacks.
        if let Err(error) = context.run_jobs() {
            outcome
                .errors
                .push(format!("JavaScript timer {timer_id} promise job: {error}"));
        }
        drain_dynamic_scripts(context, host, outcome, dynamic_script_loader, total_bytes);
    }

    host.borrow_mut().timers.advance_to(horizon);
}

fn evaluate_script(
    context: &mut Context,
    host: &Rc<RefCell<HostState>>,
    outcome: &mut ScriptOutcome,
    script: &ScriptInput,
    dispatch_load: bool,
) -> bool {
    let node_id = host.borrow_mut().id_for(&script.node);
    let current_script = format!("document.__setCurrentScript({node_id});");
    if let Err(error) = context.eval(Source::from_bytes(&current_script)) {
        outcome.errors.push(format!(
            "{}: set document.currentScript: {error}",
            script.source_url
        ));
    }

    let script_started = Instant::now();
    let succeeded = match context.eval(Source::from_bytes(&script.code)) {
        Ok(_) => {
            outcome.executed += 1;
            host.borrow_mut().executed += 1;
            if let Err(error) = context.run_jobs() {
                outcome
                    .errors
                    .push(format!("{}: promise job: {error}", script.source_url));
            }
            true
        }
        Err(error) => {
            outcome
                .errors
                .push(format!("{}: {error}", script.source_url));
            false
        }
    };
    let script_time = script_started.elapsed();
    if script_time.as_millis() >= 1 {
        outcome.diagnostics.push(format!(
            "JavaScript {:.3} ms: {}",
            script_time.as_secs_f64() * 1_000.0,
            script.source_url
        ));
    }

    if dispatch_load {
        let event_type = if succeeded { "load" } else { "error" };
        let dispatch = format!(
            "if (document.currentScript) document.currentScript.dispatchEvent(new Event('{event_type}'));"
        );
        if let Err(error) = context.eval(Source::from_bytes(&dispatch)) {
            outcome.errors.push(format!(
                "{}: dispatch {event_type} event: {error}",
                script.source_url
            ));
        }
    }
    succeeded
}

fn drain_dynamic_scripts(
    context: &mut Context,
    host: &Rc<RefCell<HostState>>,
    outcome: &mut ScriptOutcome,
    dynamic_script_loader: &mut Option<&mut DynamicScriptLoader<'_>>,
    total_bytes: &mut usize,
) {
    let Some(loader) = dynamic_script_loader.as_mut() else {
        return;
    };
    let mut executed = 0_usize;
    loop {
        let pending = std::mem::take(&mut host.borrow_mut().pending_dynamic_scripts);
        if pending.is_empty() {
            return;
        }

        for pending_script in pending {
            if executed >= MAX_DYNAMIC_SCRIPTS {
                outcome.errors.push(format!(
                    "dynamically inserted scripts exceeded the limit of {MAX_DYNAMIC_SCRIPTS}"
                ));
                return;
            }
            executed += 1;
            let code = match loader(&pending_script.source_url) {
                Ok(code) => code,
                Err(error) => {
                    outcome.errors.push(format!(
                        "{}: dynamically inserted script could not be loaded: {error}",
                        pending_script.source_url
                    ));
                    continue;
                }
            };
            if total_bytes.saturating_add(code.len()) > MAX_SCRIPT_BYTES {
                outcome.errors.push(format!(
                    "{}: skipped because the page exceeds the {} MiB JavaScript limit",
                    pending_script.source_url,
                    MAX_SCRIPT_BYTES / 1024 / 1024
                ));
                return;
            }
            *total_bytes += code.len();
            let script = ScriptInput {
                node: pending_script.node,
                source_url: pending_script.source_url,
                code,
                finish_lifecycle: false,
            };
            evaluate_script(context, host, outcome, &script, true);
        }
    }
}

const IFRAME_REALM_BOOTSTRAP: &str = r#"
globalThis.window = globalThis;
globalThis.self = globalThis;
if (typeof String.prototype.substr !== 'function') {
    Object.defineProperty(String.prototype, 'substr', {
        configurable: true,
        writable: true,
        value(start, length) {
            const string = String(this);
            const size = string.length;
            let from = Number(start) || 0;
            from = from < 0 ? Math.max(size + Math.ceil(from), 0) : Math.min(Math.floor(from), size);
            if (length === undefined) return string.slice(from);
            let count = Number(length);
            if (Number.isNaN(count) || count <= 0) return '';
            if (count !== Infinity) count = Math.floor(count);
            return string.slice(from, Math.min(from + count, size));
        }
    });
}
"#;

fn finish_host(mut outcome: ScriptOutcome, host: &Rc<RefCell<HostState>>) -> ScriptOutcome {
    let mut state = host.borrow_mut();
    outcome.mutation_count = std::mem::take(&mut state.mutation_count);
    outcome.executed = outcome.executed.max(std::mem::take(&mut state.executed));
    outcome.console.append(&mut state.console);
    outcome.diagnostics.append(&mut state.diagnostics);
    outcome.navigation_url = state.navigation_url.take();
    outcome.cookie_updates.append(&mut state.cookie_updates);
    outcome.render_requested = state.timers.take_render_request();
    outcome
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

fn host_call(_this: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let operation = argument_string(args, 0, context)?;
    let host = context
        .get_data::<HostStateLink>()
        .and_then(|link| link.0.upgrade())
        .ok_or_else(|| JsNativeError::typ().with_message("browser host is not active"))?;
    let mut host = host.borrow_mut();
    let state = &mut *host;

    match operation.as_str() {
        "document" => {
            let document = state.document.clone();
            Ok(JsValue::from(state.id_for(&document)))
        }
        "nodeType" => {
            let kind = state
                .node(argument_id(args, 1))
                .map(|node| match node.data {
                    NodeData::Element(_) => 1,
                    NodeData::Text(_) => 3,
                    NodeData::Comment(_) => 8,
                    NodeData::Document if node.id() == state.document.id() => 9,
                    NodeData::Document => 11,
                    NodeData::Doctype { .. } => 10,
                    NodeData::ProcessingInstruction { .. } => 7,
                })
                .unwrap_or_default();
            Ok(JsValue::from(kind))
        }
        "tagName" => {
            let value = state
                .node(argument_id(args, 1))
                .and_then(|node| {
                    node.tag_name().map(|tag| {
                        if node.namespace_uri() == Some("http://www.w3.org/1999/xhtml") {
                            tag.to_ascii_uppercase()
                        } else {
                            tag.to_string()
                        }
                    })
                })
                .unwrap_or_default();
            Ok(js_string(value))
        }
        "localName" => {
            let value = state
                .node(argument_id(args, 1))
                .and_then(|node| node.tag_name().map(str::to_string))
                .unwrap_or_default();
            Ok(js_string(value))
        }
        "namespaceUri" => {
            let value = state
                .node(argument_id(args, 1))
                .and_then(|node| node.namespace_uri().map(str::to_string));
            Ok(value.map_or_else(JsValue::null, js_string))
        }
        "templateContent" => {
            let contents = state.node(argument_id(args, 1)).and_then(|node| {
                node.element()
                    .and_then(|element| element.template_contents.borrow().clone())
            });
            Ok(JsValue::from(
                contents.map(|node| state.id_for(&node)).unwrap_or_default(),
            ))
        }
        "uaStyle" => {
            let property = argument_string(args, 2, context)?.to_ascii_lowercase();
            let value = state
                .node(argument_id(args, 1))
                .and_then(|node| {
                    if property == "display" && is_hidden_by_html_rendering(&node) {
                        Some("none")
                    } else {
                        node.tag_name()
                            .and_then(|tag| user_agent_style_property(tag, &property))
                    }
                })
                .unwrap_or_default();
            Ok(js_string(value.to_string()))
        }
        "parent" => {
            let parent = state
                .node(argument_id(args, 1))
                .and_then(|node| node.parent());
            Ok(JsValue::from(
                parent.map(|node| state.id_for(&node)).unwrap_or_default(),
            ))
        }
        "firstChild" => {
            let child = state
                .node(argument_id(args, 1))
                .and_then(|node| node.children.borrow().first().cloned());
            Ok(JsValue::from(
                child.map(|node| state.id_for(&node)).unwrap_or_default(),
            ))
        }
        "lastChild" => {
            let child = state
                .node(argument_id(args, 1))
                .and_then(|node| node.children.borrow().last().cloned());
            Ok(JsValue::from(
                child.map(|node| state.id_for(&node)).unwrap_or_default(),
            ))
        }
        "nextSibling" => Ok(JsValue::from(sibling_id(state, args, true))),
        "previousSibling" => Ok(JsValue::from(sibling_id(state, args, false))),
        "children" => {
            let children = state
                .node(argument_id(args, 1))
                .map(|node| node.children.borrow().clone())
                .unwrap_or_default();
            Ok(js_string(join_node_ids(state, &children, false)))
        }
        "elementChildren" => {
            let children = state
                .node(argument_id(args, 1))
                .map(|node| node.children.borrow().clone())
                .unwrap_or_default();
            Ok(js_string(join_node_ids(state, &children, true)))
        }
        "createElement" => {
            let tag_name = argument_string(args, 1, context)?;
            let node = Node::create_element_for(&state.document, &tag_name);
            Ok(JsValue::from(state.id_for(&node)))
        }
        "createText" => {
            let contents = argument_string(args, 1, context)?;
            let node = Node::create_text_for(&state.document, &contents);
            Ok(JsValue::from(state.id_for(&node)))
        }
        "createComment" => {
            let contents = argument_string(args, 1, context)?;
            let node = Node::create_comment_for(&state.document, &contents);
            Ok(JsValue::from(state.id_for(&node)))
        }
        "appendChild" => {
            let parent = state.node(argument_id(args, 1));
            let child = state.node(argument_id(args, 2));
            let changed = parent
                .zip(child.clone())
                .is_some_and(|(parent, child)| Node::append_child(&parent, child));
            if changed {
                state.record_mutation();
                if let (Some(parent), Some(child)) = (
                    state.node(argument_id(args, 1)),
                    state.node(argument_id(args, 2)),
                ) {
                    state.diagnose(format!(
                        "append {} to {}",
                        node_label(&child),
                        node_label(&parent)
                    ));
                    state.queue_dynamic_script(&child);
                }
            }
            Ok(JsValue::from(if changed {
                child.map(|node| state.id_for(&node)).unwrap_or_default()
            } else {
                0
            }))
        }
        "insertBefore" => {
            let parent = state.node(argument_id(args, 1));
            let child = state.node(argument_id(args, 2));
            let reference_id = argument_id(args, 3);
            let changed = if reference_id == 0 {
                parent
                    .zip(child.clone())
                    .is_some_and(|(parent, child)| Node::append_child(&parent, child))
            } else {
                let reference = state.node(reference_id);
                parent.zip(child.clone()).zip(reference).is_some_and(
                    |((parent, child), reference)| Node::insert_before(&parent, child, &reference),
                )
            };
            if changed {
                state.record_mutation();
                state.diagnose("insert node before sibling".into());
                if let Some(child) = child.as_ref() {
                    state.queue_dynamic_script(child);
                }
            }
            Ok(JsValue::from(if changed {
                child.map(|node| state.id_for(&node)).unwrap_or_default()
            } else {
                0
            }))
        }
        "removeChild" => {
            let parent = state.node(argument_id(args, 1));
            let child = state.node(argument_id(args, 2));
            let changed = parent
                .zip(child)
                .is_some_and(|(parent, child)| Node::remove_child(&parent, &child));
            if changed {
                state.record_mutation();
                state.diagnose("remove child node".into());
            }
            Ok(JsValue::from(changed))
        }
        "remove" => {
            let node = state.node(argument_id(args, 1));
            let changed = node.as_ref().is_some_and(|node| node.parent().is_some());
            if let Some(node) = node {
                Node::remove_from_parent(&node);
            }
            if changed {
                state.record_mutation();
                state.diagnose("remove node".into());
            }
            Ok(JsValue::from(changed))
        }
        "textGet" => {
            let value = state
                .node(argument_id(args, 1))
                .map(|node| node.text_content())
                .unwrap_or_default();
            Ok(js_string(value))
        }
        "textSet" => {
            let contents = argument_string(args, 2, context)?;
            let changed = if let Some(node) = state.node(argument_id(args, 1)) {
                Node::set_text_content(&node, &contents);
                state.register_subtree(&node);
                true
            } else {
                false
            };
            if changed {
                state.record_mutation();
                if let Some(node) = state.node(argument_id(args, 1)) {
                    state.diagnose(format!("set textContent on {}", node_label(&node)));
                }
            }
            Ok(JsValue::from(changed))
        }
        "attrGet" => {
            let name = argument_string(args, 2, context)?;
            let value = state
                .node(argument_id(args, 1))
                .and_then(|node| node.attr(&name));
            Ok(value.map_or_else(JsValue::null, js_string))
        }
        "attrSet" => {
            let name = argument_string(args, 2, context)?;
            let value = argument_string(args, 3, context)?;
            let changed = state
                .node(argument_id(args, 1))
                .is_some_and(|node| node.set_attr(&name, &value));
            if changed {
                state.record_mutation();
                if let Some(node) = state.node(argument_id(args, 1)) {
                    state.diagnose(format!("set {} on {}", name, node_label(&node)));
                    if name.eq_ignore_ascii_case("src") {
                        state.queue_dynamic_script(&node);
                    }
                }
            }
            Ok(JsValue::from(changed))
        }
        "attrRemove" => {
            let name = argument_string(args, 2, context)?;
            let changed = state
                .node(argument_id(args, 1))
                .is_some_and(|node| node.remove_attr(&name));
            if changed {
                state.record_mutation();
                if let Some(node) = state.node(argument_id(args, 1)) {
                    state.diagnose(format!("remove {} from {}", name, node_label(&node)));
                }
            }
            Ok(JsValue::from(changed))
        }
        "attrHas" => {
            let name = argument_string(args, 2, context)?;
            let present = state
                .node(argument_id(args, 1))
                .is_some_and(|node| node.attr(&name).is_some());
            Ok(JsValue::from(present))
        }
        "attrNames" => {
            let names = state
                .node(argument_id(args, 1))
                .and_then(|node| {
                    node.element().map(|element| {
                        element
                            .attrs
                            .borrow()
                            .iter()
                            .map(|attribute| attribute.name.local.to_string())
                            .collect::<Vec<_>>()
                            .join("\u{1f}")
                    })
                })
                .unwrap_or_default();
            Ok(js_string(names))
        }
        "innerHtmlGet" => {
            let value = state
                .node(argument_id(args, 1))
                .map(|node| serialize_children(&node))
                .unwrap_or_default();
            Ok(js_string(value))
        }
        "innerHtmlSet" => {
            let html = argument_string(args, 2, context)?;
            let changed = if let Some(node) = state.node(argument_id(args, 1)) {
                Node::replace_inner_html(&node, &html, true);
                state.register_subtree(&node);
                true
            } else {
                false
            };
            if changed {
                state.record_mutation();
                if let Some(node) = state.node(argument_id(args, 1)) {
                    state.diagnose(format!("replace innerHTML of {}", node_label(&node)));
                }
            }
            Ok(JsValue::from(changed))
        }
        "innerHtmlAppend" => {
            let html = argument_string(args, 2, context)?;
            let changed = if let Some(node) = state.node(argument_id(args, 1)) {
                let holder = Node::create_element_for(&state.document, "div");
                Node::replace_inner_html(&holder, &html, true);
                for child in holder.children.borrow().clone() {
                    Node::append_child(&node, child);
                }
                state.register_subtree(&node);
                true
            } else {
                false
            };
            if changed {
                state.record_mutation();
                if let Some(node) = state.node(argument_id(args, 1)) {
                    state.diagnose(format!("append innerHTML to {}", node_label(&node)));
                }
            }
            Ok(JsValue::from(changed))
        }
        "query" => {
            let selector = argument_string(args, 2, context)?;
            let node = state
                .node(argument_id(args, 1))
                .and_then(|root| query_selector_all(&root, &selector).into_iter().next());
            Ok(JsValue::from(
                node.map(|node| state.id_for(&node)).unwrap_or_default(),
            ))
        }
        "queryAll" => {
            let selector = argument_string(args, 2, context)?;
            let nodes = state
                .node(argument_id(args, 1))
                .map(|root| query_selector_all(&root, &selector))
                .unwrap_or_default();
            Ok(js_string(join_node_ids(state, &nodes, false)))
        }
        "byId" => {
            let wanted = argument_string(args, 1, context)?;
            let root = state.document.clone();
            let node = Node::descendants(&root)
                .find(|node| node.attr("id").as_deref() == Some(wanted.as_str()));
            Ok(JsValue::from(
                node.map(|node| state.id_for(&node)).unwrap_or_default(),
            ))
        }
        "documentUrl" => Ok(js_string(state.document_url.clone())),
        "cookieGet" => Ok(js_string(state.cookie_header())),
        "cookieSet" => {
            state.set_cookie(argument_string(args, 1, context)?);
            Ok(JsValue::undefined())
        }
        "userAgent" => Ok(js_string(crate::branding::USER_AGENT.to_string())),
        "resolveUrl" => {
            let value = argument_string(args, 1, context)?;
            Ok(js_string(state.resolved_url(&value)))
        }
        "navigate" => {
            let value = argument_string(args, 1, context)?;
            let resolved = state.resolved_url(&value);
            state.navigation_url = Some(resolved.clone());
            Ok(js_string(resolved))
        }
        "timerSchedule" => {
            let id = argument_id(args, 1);
            if id == 0 {
                return Err(JsNativeError::range()
                    .with_message("timer identifiers must be positive integers")
                    .into());
            }
            let delay = argument_duration(args, 2);
            let repeat = args.get(3).and_then(JsValue::as_boolean).unwrap_or(false);
            state.schedule_timer(id, delay, repeat);
            Ok(JsValue::from(id))
        }
        "timerCancel" => {
            let cancelled = state.cancel_timer(argument_id(args, 1));
            Ok(JsValue::from(cancelled))
        }
        "console" => {
            let level = argument_string(args, 1, context)?;
            let message = argument_string(args, 2, context)?;
            state.console.push(format!("{level}: {message}"));
            Ok(JsValue::undefined())
        }
        _ => Err(JsNativeError::typ()
            .with_message(format!("unsupported browser host operation: {operation}"))
            .into()),
    }
}

fn argument_string(arguments: &[JsValue], index: usize, context: &mut Context) -> JsResult<String> {
    match arguments.get(index) {
        Some(value) => Ok(value.to_string(context)?.to_std_string_escaped()),
        None => Ok(String::new()),
    }
}

fn argument_id(arguments: &[JsValue], index: usize) -> u32 {
    arguments
        .get(index)
        .and_then(JsValue::as_number)
        .filter(|value| value.is_finite() && *value >= 0.0 && *value <= f64::from(u32::MAX))
        .map(|value| value as u32)
        .unwrap_or_default()
}

fn argument_duration(arguments: &[JsValue], index: usize) -> Duration {
    let milliseconds = arguments
        .get(index)
        .and_then(JsValue::as_number)
        .filter(|value| value.is_finite() && *value > 0.0)
        .unwrap_or(0.0);
    Duration::try_from_secs_f64(milliseconds / 1_000.0).unwrap_or(Duration::MAX)
}

fn js_string(value: String) -> JsValue {
    JsValue::from(JsString::from(value))
}

fn node_label(node: &NodeRef) -> String {
    match &node.data {
        NodeData::Document => "#document".into(),
        NodeData::Text(_) => "#text".into(),
        NodeData::Comment(_) => "#comment".into(),
        _ => node
            .tag_name()
            .map(|tag| format!("<{tag}>"))
            .unwrap_or_else(|| "#node".into()),
    }
}

fn join_node_ids(state: &mut HostState, nodes: &[NodeRef], elements_only: bool) -> String {
    nodes
        .iter()
        .filter(|node| !elements_only || node.element().is_some())
        .map(|node| state.id_for(node).to_string())
        .collect::<Vec<_>>()
        .join(",")
}

fn sibling_id(state: &mut HostState, arguments: &[JsValue], next: bool) -> u32 {
    let Some(node) = state.node(argument_id(arguments, 1)) else {
        return 0;
    };
    let Some(parent) = node.parent() else {
        return 0;
    };
    let children = parent.children.borrow();
    let Some(index) = children.iter().position(|child| child.id() == node.id()) else {
        return 0;
    };
    let sibling = if next {
        children.get(index + 1)
    } else {
        index.checked_sub(1).and_then(|index| children.get(index))
    }
    .cloned();
    drop(children);
    sibling.map(|node| state.id_for(&node)).unwrap_or_default()
}

fn query_selector_all(root: &NodeRef, selector: &str) -> Vec<NodeRef> {
    let groups = selector
        .split(',')
        .map(str::trim)
        .filter(|group| !group.is_empty())
        .collect::<Vec<_>>();
    let mut result: Vec<NodeRef> = Vec::new();
    for node in Node::descendants(root).skip(1) {
        if node.element().is_none() {
            continue;
        }
        if groups.iter().any(|group| matches_selector(&node, group))
            && !result.iter().any(|existing| existing.id() == node.id())
        {
            result.push(node);
        }
    }
    result
}

fn matches_selector(node: &NodeRef, selector: &str) -> bool {
    let parts = selector.split_ascii_whitespace().collect::<Vec<_>>();
    let Some(last) = parts.last() else {
        return false;
    };
    if !matches_compound_selector(node, last) {
        return false;
    }
    let mut ancestor = node.parent();
    for wanted in parts[..parts.len() - 1].iter().rev() {
        if *wanted == ">" {
            continue;
        }
        let mut matched = None;
        while let Some(candidate) = ancestor {
            ancestor = candidate.parent();
            if matches_compound_selector(&candidate, wanted) {
                matched = Some(candidate);
                break;
            }
        }
        if matched.is_none() {
            return false;
        }
    }
    true
}

fn matches_compound_selector(node: &NodeRef, selector: &str) -> bool {
    let selector = selector.trim_matches('>');
    if selector == "*" {
        return true;
    }

    let bytes = selector.as_bytes();
    let mut index = 0;
    let tag_end = selector
        .find(['#', '.', '[', ':'])
        .unwrap_or(selector.len());
    if tag_end > 0 {
        let tag = &selector[..tag_end];
        if node
            .tag_name()
            .is_none_or(|actual| !actual.eq_ignore_ascii_case(tag))
        {
            return false;
        }
        index = tag_end;
    }

    while index < bytes.len() {
        match bytes[index] {
            b'#' | b'.' => {
                let marker = bytes[index];
                index += 1;
                let end = selector[index..]
                    .find(['#', '.', '[', ':'])
                    .map(|offset| index + offset)
                    .unwrap_or(selector.len());
                let value = &selector[index..end];
                if marker == b'#' {
                    if node.attr("id").as_deref() != Some(value) {
                        return false;
                    }
                } else if !node.has_class(value) {
                    return false;
                }
                index = end;
            }
            b'[' => {
                let Some(offset) = selector[index + 1..].find(']') else {
                    return false;
                };
                let end = index + 1 + offset;
                let expression = selector[index + 1..end].trim();
                if let Some((name, value)) = expression.split_once('=') {
                    let value = value.trim().trim_matches(['\'', '"']);
                    if node.attr(name.trim()).as_deref() != Some(value) {
                        return false;
                    }
                } else if node.attr(expression).is_none() {
                    return false;
                }
                index = end + 1;
            }
            b':' => {
                let pseudo = &selector[index + 1..];
                match pseudo {
                    "first-child" => {
                        let Some(parent) = node.parent() else {
                            return false;
                        };
                        if parent
                            .children
                            .borrow()
                            .iter()
                            .find(|child| child.element().is_some())
                            .is_none_or(|child| child.id() != node.id())
                        {
                            return false;
                        }
                    }
                    "last-child" => {
                        let Some(parent) = node.parent() else {
                            return false;
                        };
                        if parent
                            .children
                            .borrow()
                            .iter()
                            .rev()
                            .find(|child| child.element().is_some())
                            .is_none_or(|child| child.id() != node.id())
                        {
                            return false;
                        }
                    }
                    _ => return false,
                }
                index = selector.len();
            }
            _ => return false,
        }
    }
    true
}

fn serialize_children(node: &NodeRef) -> String {
    let mut output = String::new();
    let target = node
        .element()
        .and_then(|element| element.template_contents.borrow().clone())
        .unwrap_or_else(|| node.clone());
    for child in target.children.borrow().iter() {
        serialize_node(child, &mut output);
    }
    output
}

fn serialize_node(node: &NodeRef, output: &mut String) {
    match &node.data {
        NodeData::Element(element) => {
            let tag = element.name.local.as_ref();
            output.push('<');
            output.push_str(tag);
            for attribute in element.attrs.borrow().iter() {
                output.push(' ');
                output.push_str(attribute.name.local.as_ref());
                output.push_str("=\"");
                escape_html(&attribute.value, output, true);
                output.push('"');
            }
            output.push('>');
            for child in node.children.borrow().iter() {
                serialize_node(child, output);
            }
            output.push_str("</");
            output.push_str(tag);
            output.push('>');
        }
        NodeData::Text(text) => escape_html(&text.borrow(), output, false),
        NodeData::Comment(comment) => {
            output.push_str("<!--");
            output.push_str(comment);
            output.push_str("-->");
        }
        _ => {}
    }
}

fn escape_html(value: &str, output: &mut String, attribute: bool) {
    for character in value.chars() {
        match character {
            '&' => output.push_str("&amp;"),
            '<' => output.push_str("&lt;"),
            '>' => output.push_str("&gt;"),
            '"' if attribute => output.push_str("&quot;"),
            character => output.push(character),
        }
    }
}

const BROWSER_BOOTSTRAP: &str = r#"
(() => {
    'use strict';
    const host = (...args) => __hostCall(...args);
    const isolatedIframeWindow = globalThis.__iframeWindow;
    delete globalThis.__iframeWindow;
    if (typeof String.prototype.substr !== 'function') {
        Object.defineProperty(String.prototype, 'substr', {
            configurable: true,
            writable: true,
            value(start, length) {
                const string = String(this);
                const size = string.length;
                let from = Number(start) || 0;
                from = from < 0 ? Math.max(size + Math.ceil(from), 0) : Math.min(Math.floor(from), size);
                if (length === undefined) return string.slice(from);
                let count = Number(length);
                if (Number.isNaN(count) || count <= 0) return '';
                if (count !== Infinity) count = Math.floor(count);
                return string.slice(from, Math.min(from + count, size));
            }
        });
    }
    const cache = new Map();
    const list = value => {
        if (!value) return [];
        const result = value.split(',').filter(Boolean).map(id => wrap(Number(id)));
        result.item = index => result[index] || null;
        return result;
    };

    class Event {
        constructor(type, init = {}) {
            this.type = String(type);
            this.bubbles = !!init.bubbles;
            this.cancelable = !!init.cancelable;
            this.defaultPrevented = false;
            this.target = null;
            this.currentTarget = null;
            this.timeStamp = Date.now();
        }
        preventDefault() { if (this.cancelable) this.defaultPrevented = true; }
        stopPropagation() { this.__stopped = true; }
        stopImmediatePropagation() { this.__stopped = this.__immediate = true; }
    }
    class CustomEvent extends Event {
        constructor(type, init = {}) {
            super(type, init);
            this.detail = init.detail === undefined ? null : init.detail;
        }
    }
    class MessageEvent extends Event {
        constructor(type, init = {}) {
            super(type, init);
            this.data = init.data === undefined ? null : init.data;
            this.origin = init.origin === undefined ? '' : String(init.origin);
            this.lastEventId = init.lastEventId === undefined ? '' : String(init.lastEventId);
            this.source = init.source === undefined ? null : init.source;
            this.ports = Object.freeze([...(init.ports || [])]);
        }
        initMessageEvent(type, bubbles = false, cancelable = false, data = null, origin = '', lastEventId = '', source = null, ports = []) {
            this.type = String(type);
            this.bubbles = !!bubbles;
            this.cancelable = !!cancelable;
            this.data = data;
            this.origin = String(origin);
            this.lastEventId = String(lastEventId);
            this.source = source;
            this.ports = Object.freeze([...ports]);
        }
    }
    class ToggleEvent extends Event {
        constructor(type, init = {}) {
            super(type, init);
            this.oldState = init.oldState === undefined ? '' : String(init.oldState);
            this.newState = init.newState === undefined ? '' : String(init.newState);
            this.source = init.source === undefined ? null : init.source;
        }
    }
    class DOMException extends Error {
        constructor(message = '', name = 'Error') {
            super(String(message));
            this.name = String(name);
            this.code = 0;
        }
    }
    const listenerStore = new WeakMap();
    class EventTarget {
        addEventListener(type, callback) {
            if (typeof callback !== 'function' && !(callback && typeof callback.handleEvent === 'function')) return;
            let listeners = listenerStore.get(this);
            if (!listeners) listenerStore.set(this, listeners = new Map());
            const bucket = listeners.get(String(type)) || [];
            if (!bucket.includes(callback)) bucket.push(callback);
            listeners.set(String(type), bucket);
        }
        removeEventListener(type, callback) {
            const bucket = listenerStore.get(this)?.get(String(type));
            if (!bucket) return;
            const index = bucket.indexOf(callback);
            if (index >= 0) bucket.splice(index, 1);
        }
        dispatchEvent(event) {
            if (!(event instanceof Event)) event = new Event(String(event));
            const dispatchTarget = this.__eventTargetProxy || this;
            event.target ||= dispatchTarget;
            event.currentTarget = dispatchTarget;
            const bucket = listenerStore.get(this)?.get(event.type) || [];
            for (const callback of [...bucket]) {
                if (typeof callback === 'function') callback.call(dispatchTarget, event);
                else callback.handleEvent(event);
                if (event.__immediate) break;
            }
            const handler = dispatchTarget['on' + event.type];
            if (!event.__immediate && typeof handler === 'function') handler.call(dispatchTarget, event);
            return !event.defaultPrevented;
        }
    }

    class Node extends EventTarget {
        constructor(id) {
            super();
            this.__id = id;
        }
        get nodeType() { return host('nodeType', this.__id); }
        get nodeName() {
            return this.nodeType === 1 ? host('tagName', this.__id) :
                this.nodeType === 9 ? '#document' :
                this.nodeType === 11 ? '#document-fragment' :
                this.nodeType === 3 ? '#text' : '#comment';
        }
        get ownerDocument() { return this.nodeType === 9 ? null : document; }
        get parentNode() { return wrap(host('parent', this.__id)); }
        get parentElement() { const parent = this.parentNode; return parent?.nodeType === 1 ? parent : null; }
        get firstChild() { return wrap(host('firstChild', this.__id)); }
        get lastChild() { return wrap(host('lastChild', this.__id)); }
        get nextSibling() { return wrap(host('nextSibling', this.__id)); }
        get previousSibling() { return wrap(host('previousSibling', this.__id)); }
        get childNodes() { return list(host('children', this.__id)); }
        get children() { return list(host('elementChildren', this.__id)); }
        get firstElementChild() { return this.children[0] || null; }
        get lastElementChild() { const children = this.children; return children[children.length - 1] || null; }
        get childElementCount() { return this.children.length; }
        get textContent() { return host('textGet', this.__id); }
        set textContent(value) { host('textSet', this.__id, value == null ? '' : String(value)); }
        get isConnected() {
            let node = this;
            while (node) {
                if (node.nodeType === 9) return true;
                node = node.parentNode;
            }
            return false;
        }
        appendChild(child) {
            if (!(child instanceof Node)) throw new TypeError('appendChild requires a Node');
            return wrap(host('appendChild', this.__id, child.__id));
        }
        append(...items) {
            for (const item of items) this.appendChild(item instanceof Node ? item : document.createTextNode(String(item)));
        }
        prepend(...items) {
            let reference = this.firstChild;
            for (const item of items) {
                const node = item instanceof Node ? item : document.createTextNode(String(item));
                this.insertBefore(node, reference);
                if (!reference) reference = node.nextSibling;
            }
        }
        insertBefore(child, reference) {
            if (!(child instanceof Node)) throw new TypeError('insertBefore requires a Node');
            if (reference != null && !(reference instanceof Node)) throw new TypeError('reference must be a Node');
            return wrap(host('insertBefore', this.__id, child.__id, reference?.__id || 0));
        }
        removeChild(child) {
            if (!(child instanceof Node) || !host('removeChild', this.__id, child.__id)) throw new Error('node is not a child');
            return child;
        }
        remove() { host('remove', this.__id); }
        contains(other) {
            for (let node = other; node; node = node.parentNode) if (node === this) return true;
            return false;
        }
        hasChildNodes() { return !!this.firstChild; }
        querySelector(selector) { return wrap(host('query', this.__id, String(selector))); }
        querySelectorAll(selector) { return list(host('queryAll', this.__id, String(selector))); }
    }

    class Text extends Node {
        get data() { return this.textContent; }
        set data(value) { this.textContent = value; }
        get nodeValue() { return this.data; }
        set nodeValue(value) { this.data = value == null ? '' : String(value); }
        get length() { return this.data.length; }
    }

    class Comment extends Text {}
    class DocumentFragment extends Node {}

    class DOMTokenList {
        constructor(element, attribute) { this.element = element; this.attribute = attribute; }
        _tokens() { return (this.element.getAttribute(this.attribute) || '').split(/\s+/).filter(Boolean); }
        _set(tokens) { this.element.setAttribute(this.attribute, [...new Set(tokens)].join(' ')); }
        contains(token) { return this._tokens().includes(String(token)); }
        add(...tokens) { this._set(this._tokens().concat(tokens.map(String))); }
        remove(...tokens) { const remove = new Set(tokens.map(String)); this._set(this._tokens().filter(token => !remove.has(token))); }
        toggle(token, force) {
            token = String(token);
            const present = this.contains(token);
            if (force === true || (!present && force !== false)) { this.add(token); return true; }
            if (present) this.remove(token);
            return false;
        }
        replace(oldToken, newToken) {
            const tokens = this._tokens();
            const index = tokens.indexOf(String(oldToken));
            if (index < 0) return false;
            tokens[index] = String(newToken);
            this._set(tokens);
            return true;
        }
        get value() { return this.element.getAttribute(this.attribute) || ''; }
        set value(value) { this.element.setAttribute(this.attribute, value); }
        get length() { return this._tokens().length; }
        item(index) { return this._tokens()[index] || null; }
        [Symbol.iterator]() { return this._tokens()[Symbol.iterator](); }
        toString() { return this.value; }
    }

    class CSSStyleDeclaration {
        constructor(element) { this.element = element; }
        _map() {
            const map = new Map();
            for (const declaration of (this.element.getAttribute('style') || '').split(';')) {
                const split = declaration.indexOf(':');
                if (split > 0) map.set(declaration.slice(0, split).trim().toLowerCase(), declaration.slice(split + 1).trim());
            }
            return map;
        }
        _write(map) { this.element.setAttribute('style', [...map].map(([name, value]) => name + ': ' + value).join('; ')); }
        get cssText() { return this.element.getAttribute('style') || ''; }
        set cssText(value) { this.element.setAttribute('style', String(value)); }
        getPropertyValue(name) { return this._map().get(String(name).toLowerCase()) || ''; }
        setProperty(name, value, priority = '') {
            const map = this._map();
            map.set(String(name).toLowerCase(), String(value) + (priority ? ' !' + priority : ''));
            this._write(map);
        }
        removeProperty(name) {
            const map = this._map();
            const old = map.get(String(name).toLowerCase()) || '';
            map.delete(String(name).toLowerCase());
            this._write(map);
            return old;
        }
    }
    const styleProxy = element => new Proxy(new CSSStyleDeclaration(element), {
        get(target, property) {
            if (property in target) {
                const value = target[property];
                return typeof value === 'function' ? value.bind(target) : value;
            }
            return target.getPropertyValue(String(property).replace(/[A-Z]/g, match => '-' + match.toLowerCase()));
        },
        set(target, property, value) {
            if (property === 'cssText') target.cssText = value;
            else target.setProperty(String(property).replace(/[A-Z]/g, match => '-' + match.toLowerCase()), value);
            return true;
        }
    });

    class Element extends Node {
        get tagName() { return host('tagName', this.__id); }
        get localName() { return host('localName', this.__id); }
        get namespaceURI() { return host('namespaceUri', this.__id); }
        get prefix() { return null; }
        get id() { return this.getAttribute('id') || ''; }
        set id(value) { this.setAttribute('id', value); }
        get className() { return this.getAttribute('class') || ''; }
        set className(value) { this.setAttribute('class', value); }
        get classList() { return this.__classList ||= new DOMTokenList(this, 'class'); }
        get style() { return this.__style ||= styleProxy(this); }
        get innerHTML() { return host('innerHtmlGet', this.__id); }
        set innerHTML(value) { host('innerHtmlSet', this.__id, value == null ? '' : String(value)); }
        get outerHTML() { return '<' + this.localName + '>' + this.innerHTML + '</' + this.localName + '>'; }
        getAttribute(name) { return host('attrGet', this.__id, String(name)); }
        setAttribute(name, value) { host('attrSet', this.__id, String(name), String(value)); }
        removeAttribute(name) { host('attrRemove', this.__id, String(name)); }
        hasAttribute(name) { return host('attrHas', this.__id, String(name)); }
        toggleAttribute(name, force) {
            const present = this.hasAttribute(name);
            if (force === true || (!present && force !== false)) { this.setAttribute(name, ''); return true; }
            if (present) this.removeAttribute(name);
            return false;
        }
        getAttributeNames() {
            const names = host('attrNames', this.__id);
            return names ? names.split('\u001f') : [];
        }
        get attributes() {
            const element = this;
            const attributes = this.getAttributeNames().map(name => ({
                name,
                nodeName: name,
                get value() { return element.getAttribute(name) || ''; },
                set value(value) { element.setAttribute(name, value); },
                get nodeValue() { return this.value; },
                set nodeValue(value) { this.value = value; },
                ownerElement: element,
                specified: true
            }));
            attributes.item = index => attributes[index] || null;
            attributes.getNamedItem = name => attributes.find(attribute => attribute.name === String(name)) || null;
            return attributes;
        }
        matches(selector) { return this.parentNode?.querySelectorAll(selector).includes(this) || false; }
        closest(selector) {
            for (let node = this; node?.nodeType === 1; node = node.parentElement) if (node.matches(selector)) return node;
            return null;
        }
        getElementsByTagName(name) { return this.querySelectorAll(String(name)); }
        getElementsByClassName(name) { return this.querySelectorAll('.' + String(name).trim().replace(/\s+/g, '.')); }
        insertAdjacentHTML(position, html) {
            position = String(position).toLowerCase();
            if (position === 'beforeend') host('innerHtmlAppend', this.__id, String(html));
            else if (position === 'afterbegin') this.innerHTML = String(html) + this.innerHTML;
            else if (position === 'beforebegin' && this.parentNode) {
                const holder = document.createElement('div'); holder.innerHTML = String(html);
                for (const child of [...holder.childNodes]) this.parentNode.insertBefore(child, this);
            } else if (position === 'afterend' && this.parentNode) {
                const holder = document.createElement('div'); holder.innerHTML = String(html);
                let reference = this.nextSibling;
                for (const child of [...holder.childNodes]) this.parentNode.insertBefore(child, reference);
            }
        }
        insertAdjacentText(position, text) { this.insertAdjacentHTML(position, String(text).replace(/&/g, '&amp;').replace(/</g, '&lt;')); }
        get href() { const value = this.getAttribute('href'); return value == null ? '' : host('resolveUrl', value); }
        set href(value) { this.setAttribute('href', value); }
        get src() { const value = this.getAttribute('src'); return value == null ? '' : host('resolveUrl', value); }
        set src(value) { this.setAttribute('src', value); }
        get value() { return this.getAttribute('value') || ''; }
        set value(value) { this.setAttribute('value', value); }
        get name() { return this.getAttribute('name') || ''; }
        set name(value) { this.setAttribute('name', value); }
        get type() { return this.getAttribute('type') || ''; }
        set type(value) { this.setAttribute('type', value); }
        get checked() { return this.hasAttribute('checked'); }
        set checked(value) { this.toggleAttribute('checked', !!value); }
        get disabled() { return this.hasAttribute('disabled'); }
        set disabled(value) { this.toggleAttribute('disabled', !!value); }
        get hidden() { return this.hasAttribute('hidden'); }
        set hidden(value) { this.toggleAttribute('hidden', !!value); }
        get dataset() {
            const element = this;
            return new Proxy({}, {
                get(_, property) { return element.getAttribute('data-' + String(property).replace(/[A-Z]/g, match => '-' + match.toLowerCase())); },
                set(_, property, value) { element.setAttribute('data-' + String(property).replace(/[A-Z]/g, match => '-' + match.toLowerCase()), value); return true; }
            });
        }
        get contentWindow() { return this.localName === 'iframe' ? iframeWindow : null; }
        get contentDocument() { return this.localName === 'iframe' ? iframeDocument : null; }
        click() {
            const allowed = this.dispatchEvent(new Event('click', { bubbles: true, cancelable: true }));
            if (allowed && this.localName === 'summary') {
                const details = this.parentElement;
                const firstSummary = details?.children.find(child => child.localName === 'summary');
                if (details instanceof HTMLDetailsElement && firstSummary === this) details.open = !details.open;
            }
        }
        focus() { document.activeElement = this; this.dispatchEvent(new Event('focus')); }
        blur() { if (document.activeElement === this) document.activeElement = document.body; this.dispatchEvent(new Event('blur')); }
        get clientWidth() { return 0; }
        get clientHeight() { return 0; }
        get offsetWidth() { return 0; }
        get offsetHeight() { return 0; }
        getBoundingClientRect() { return { x: 0, y: 0, top: 0, left: 0, right: 0, bottom: 0, width: 0, height: 0, toJSON() { return this; } }; }
    }

    const dataPropertyName = attribute => attribute.slice(5).replace(/-([a-z])/g, (_, letter) => letter.toUpperCase());
    const dataAttributeName = property => {
        property = String(property);
        if (/-[a-z]/.test(property)) throw new SyntaxError('dataset property names cannot contain a dash followed by a lowercase letter');
        return 'data-' + property.replace(/[A-Z]/g, letter => '-' + letter.toLowerCase());
    };
    class DOMStringMap {}
    const datasetFor = element => new Proxy(new DOMStringMap(), {
        get(target, property, receiver) {
            if (typeof property !== 'string' || property in target) return Reflect.get(target, property, receiver);
            const value = element.getAttribute(dataAttributeName(property));
            return value == null ? undefined : value;
        },
        set(_target, property, value) {
            if (typeof property !== 'string') return false;
            element.setAttribute(dataAttributeName(property), String(value));
            return true;
        },
        deleteProperty(_target, property) {
            if (typeof property === 'string') element.removeAttribute(dataAttributeName(property));
            return true;
        },
        has(target, property) {
            return property in target || (typeof property === 'string' && element.hasAttribute(dataAttributeName(property)));
        },
        ownKeys() {
            return element.getAttributeNames()
                .filter(name => name.startsWith('data-') && !/[A-Z]/.test(name.slice(5)))
                .map(dataPropertyName);
        },
        getOwnPropertyDescriptor(_target, property) {
            if (typeof property !== 'string' || !element.hasAttribute(dataAttributeName(property))) return undefined;
            return { configurable: true, enumerable: true, writable: true, value: element.getAttribute(dataAttributeName(property)) };
        }
    });
    class HTMLElement extends Element {
        get dataset() { return this.__dataset ||= datasetFor(this); }
    }
    Object.defineProperties(HTMLElement.prototype, {
        translate: {
            configurable: true,
            get() {
                const value = this.getAttribute('translate');
                if (value == null || value === '') return this.parentElement?.translate ?? true;
                return value.toLowerCase() !== 'no';
            },
            set(value) { this.setAttribute('translate', value ? 'yes' : 'no'); }
        },
        accessKey: {
            configurable: true,
            get() { return this.getAttribute('accesskey') || ''; },
            set(value) { this.setAttribute('accesskey', value); }
        },
        accessKeyLabel: {
            configurable: true,
            get() { return ''; }
        }
    });
    class HTMLUnknownElement extends HTMLElement {}
    class HTMLTimeElement extends HTMLElement {
        get dateTime() { return this.getAttribute('datetime') || ''; }
        set dateTime(value) { this.setAttribute('datetime', value); }
    }
    class HTMLDataElement extends HTMLElement {
        get value() { return this.getAttribute('value') || ''; }
        set value(value) { this.setAttribute('value', value); }
    }
    class HTMLAnchorElement extends HTMLElement {
        get target() { return this.getAttribute('target') || ''; }
        set target(value) { this.setAttribute('target', value); }
        get download() { return this.getAttribute('download') || ''; }
        set download(value) { this.setAttribute('download', value); }
        get ping() { return this.getAttribute('ping') || ''; }
        set ping(value) { this.setAttribute('ping', value); }
        get rel() { return this.getAttribute('rel') || ''; }
        set rel(value) { this.setAttribute('rel', value); }
        get relList() { return this.__relList ||= new DOMTokenList(this, 'rel'); }
        get hreflang() { return this.getAttribute('hreflang') || ''; }
        set hreflang(value) { this.setAttribute('hreflang', value); }
        get type() { return this.getAttribute('type') || ''; }
        set type(value) { this.setAttribute('type', value); }
        get referrerPolicy() { return this.getAttribute('referrerpolicy') || ''; }
        set referrerPolicy(value) { this.setAttribute('referrerpolicy', value); }
        get text() { return this.textContent; }
        set text(value) { this.textContent = String(value); }
    }
    class HTMLDetailsElement extends HTMLElement {
        get open() { return this.hasAttribute('open'); }
        set open(value) {
            const wasOpen = this.open;
            const isOpen = !!value;
            if (wasOpen === isOpen) return;
            this.toggleAttribute('open', isOpen);
            setTimeout(() => this.dispatchEvent(new ToggleEvent('toggle', {
                oldState: wasOpen ? 'open' : 'closed',
                newState: isOpen ? 'open' : 'closed'
            })), 0);
        }
    }
    class HTMLDialogElement extends HTMLElement {
        constructor(id) {
            super(id);
            this.returnValue = '';
            this.__isModal = false;
        }
        get open() { return this.hasAttribute('open'); }
        set open(value) { this.toggleAttribute('open', !!value); }
        get closedBy() { return this.getAttribute('closedby') || 'none'; }
        set closedBy(value) { this.setAttribute('closedby', value); }
        show() {
            if (this.open) return;
            const event = new ToggleEvent('beforetoggle', {
                cancelable: true, oldState: 'closed', newState: 'open', source: this
            });
            if (!this.dispatchEvent(event)) return;
            this.open = true;
            this.focus();
            setTimeout(() => this.dispatchEvent(new ToggleEvent('toggle', {
                oldState: 'closed', newState: 'open', source: this
            })), 0);
        }
        showModal() {
            if (!this.isConnected) throw new DOMException('Dialog is not connected to a document', 'InvalidStateError');
            if (this.open) {
                if (!this.__isModal) throw new DOMException('Dialog is already open non-modally', 'InvalidStateError');
                return;
            }
            this.__isModal = true;
            this.show();
        }
        close(returnValue) {
            if (!this.open) return;
            if (returnValue !== undefined) this.returnValue = String(returnValue);
            this.__isModal = false;
            this.open = false;
            setTimeout(() => this.dispatchEvent(new Event('close')), 0);
        }
        requestClose(returnValue) {
            if (!this.open) return;
            if (this.dispatchEvent(new Event('cancel', { cancelable: true }))) this.close(returnValue);
        }
    }
    class HTMLScriptElement extends HTMLElement {
        get async() { return this.hasAttribute('async'); }
        set async(value) { this.toggleAttribute('async', !!value); }
        get defer() { return this.hasAttribute('defer'); }
        set defer(value) { this.toggleAttribute('defer', !!value); }
        get text() { return this.textContent; }
        set text(value) { this.textContent = String(value); }
    }
    class HTMLImageElement extends HTMLElement {
        get srcset() { return this.getAttribute('srcset') || ''; }
        set srcset(value) { this.setAttribute('srcset', value); }
        get sizes() { return this.getAttribute('sizes') || ''; }
        set sizes(value) { this.setAttribute('sizes', value); }
    }
    class HTMLPictureElement extends HTMLElement {}
    class HTMLSourceElement extends HTMLElement {
        get srcset() { return this.getAttribute('srcset') || ''; }
        set srcset(value) { this.setAttribute('srcset', value); }
        get sizes() { return this.getAttribute('sizes') || ''; }
        set sizes(value) { this.setAttribute('sizes', value); }
        get media() { return this.getAttribute('media') || ''; }
        set media(value) { this.setAttribute('media', value); }
    }
    class HTMLInputElement extends HTMLElement {
        get placeholder() { return this.getAttribute('placeholder') || ''; }
        set placeholder(value) { this.setAttribute('placeholder', value); }
        get form() { return associatedForm(this); }
        get selectionStart() { return this.__selectionStart ?? 0; }
        set selectionStart(value) { this.__selectionStart = Math.max(0, Number(value) || 0); }
        get selectionEnd() { return this.__selectionEnd ?? this.value.length; }
        set selectionEnd(value) { this.__selectionEnd = Math.max(0, Number(value) || 0); }
        get selectionDirection() { return this.__selectionDirection || 'none'; }
        set selectionDirection(value) {
            value = String(value);
            this.__selectionDirection = value === 'forward' || value === 'backward' ? value : 'none';
        }
        setSelectionRange(start, end, direction = 'none') {
            this.selectionStart = start;
            this.selectionEnd = Math.max(this.selectionStart, Number(end) || 0);
            this.selectionDirection = direction;
        }
        select() { this.setSelectionRange(0, this.value.length); }
        get indeterminate() { return !!this.__indeterminate; }
        set indeterminate(value) { this.__indeterminate = !!value; }
        get list() {
            const id = this.getAttribute('list');
            const candidate = id ? document.getElementById(id) : null;
            return candidate instanceof HTMLDataListElement ? candidate : null;
        }
        get min() { return this.getAttribute('min') || ''; }
        set min(value) { this.setAttribute('min', value); }
        get max() { return this.getAttribute('max') || ''; }
        set max(value) { this.setAttribute('max', value); }
        get step() { return this.getAttribute('step') || ''; }
        set step(value) { this.setAttribute('step', value); }
        get pattern() { return this.getAttribute('pattern') || ''; }
        set pattern(value) { this.setAttribute('pattern', value); }
        get required() { return this.hasAttribute('required'); }
        set required(value) { this.toggleAttribute('required', !!value); }
        get autofocus() { return this.hasAttribute('autofocus'); }
        set autofocus(value) { this.toggleAttribute('autofocus', !!value); }
        get autocomplete() { return this.getAttribute('autocomplete') || ''; }
        set autocomplete(value) { this.setAttribute('autocomplete', value); }
        get multiple() { return this.hasAttribute('multiple'); }
        set multiple(value) { this.toggleAttribute('multiple', !!value); }
        get dirName() { return this.getAttribute('dirname') || ''; }
        set dirName(value) { this.setAttribute('dirname', value); }
        get formAction() {
            const value = this.getAttribute('formaction');
            return value == null ? '' : host('resolveUrl', value);
        }
        set formAction(value) { this.setAttribute('formaction', value); }
        get formEnctype() { return this.getAttribute('formenctype') || ''; }
        set formEnctype(value) { this.setAttribute('formenctype', value); }
        get formMethod() { return this.getAttribute('formmethod') || ''; }
        set formMethod(value) { this.setAttribute('formmethod', value); }
        get formNoValidate() { return this.hasAttribute('formnovalidate'); }
        set formNoValidate(value) { this.toggleAttribute('formnovalidate', !!value); }
        get formTarget() { return this.getAttribute('formtarget') || ''; }
        set formTarget(value) { this.setAttribute('formtarget', value); }
        get labels() { return labelsFor(this); }
        get willValidate() { return !this.disabled && !['hidden', 'button', 'reset'].includes(this.type); }
        get validity() { return validityFor(this); }
        get validationMessage() { return this.validity.valid ? '' : (this.__customValidity || 'Please enter a valid value.'); }
        setCustomValidity(message) { this.__customValidity = String(message); }
        checkValidity() { return checkControlValidity(this); }
        reportValidity() { return this.checkValidity(); }
    }
    class HTMLTextAreaElement extends HTMLElement {
        get placeholder() { return this.getAttribute('placeholder') || ''; }
        set placeholder(value) { this.setAttribute('placeholder', value); }
        get form() { return associatedForm(this); }
        get value() { return this.__value ?? this.textContent; }
        set value(value) { this.__value = String(value); }
        get defaultValue() { return this.textContent; }
        set defaultValue(value) { this.textContent = String(value); }
        get minLength() { return reflectedInteger(this, 'minlength', -1); }
        set minLength(value) { this.setAttribute('minlength', String(Math.trunc(Number(value)))); }
        get maxLength() { return reflectedInteger(this, 'maxlength', -1); }
        set maxLength(value) { this.setAttribute('maxlength', String(Math.trunc(Number(value)))); }
        get wrap() { return (this.getAttribute('wrap') || 'soft').toLowerCase() === 'hard' ? 'hard' : 'soft'; }
        set wrap(value) { this.setAttribute('wrap', value); }
        get required() { return this.hasAttribute('required'); }
        set required(value) { this.toggleAttribute('required', !!value); }
        get labels() { return labelsFor(this); }
        get willValidate() { return !this.disabled; }
        get validity() { return validityFor(this); }
        get validationMessage() { return this.validity.valid ? '' : (this.__customValidity || 'Please enter a valid value.'); }
        setCustomValidity(message) { this.__customValidity = String(message); }
        checkValidity() { return checkControlValidity(this); }
        reportValidity() { return this.checkValidity(); }
    }
    class HTMLOrderedListElement extends HTMLElement {
        get reversed() { return this.hasAttribute('reversed'); }
        set reversed(value) { this.toggleAttribute('reversed', !!value); }
    }
    class HTMLSelectElement extends HTMLElement {
        get form() { return associatedForm(this); }
        get required() { return this.hasAttribute('required'); }
        set required(value) { this.toggleAttribute('required', !!value); }
        get labels() { return labelsFor(this); }
        get willValidate() { return !this.disabled; }
        get validity() { return validityFor(this); }
        get validationMessage() { return this.validity.valid ? '' : (this.__customValidity || 'Please select an item.'); }
        setCustomValidity(message) { this.__customValidity = String(message); }
        checkValidity() { return checkControlValidity(this); }
        reportValidity() { return this.checkValidity(); }
    }
    class HTMLButtonElement extends HTMLElement {
        get form() { return associatedForm(this); }
        get labels() { return labelsFor(this); }
        get formAction() {
            const value = this.getAttribute('formaction');
            return value == null ? '' : host('resolveUrl', value);
        }
        set formAction(value) { this.setAttribute('formaction', value); }
        get formEnctype() { return this.getAttribute('formenctype') || ''; }
        set formEnctype(value) { this.setAttribute('formenctype', value); }
        get formMethod() { return this.getAttribute('formmethod') || ''; }
        set formMethod(value) { this.setAttribute('formmethod', value); }
        get formNoValidate() { return this.hasAttribute('formnovalidate'); }
        set formNoValidate(value) { this.toggleAttribute('formnovalidate', !!value); }
        get formTarget() { return this.getAttribute('formtarget') || ''; }
        set formTarget(value) { this.setAttribute('formtarget', value); }
    }
    class HTMLLabelElement extends HTMLElement {
        get htmlFor() { return this.getAttribute('for') || ''; }
        set htmlFor(value) { this.setAttribute('for', value); }
        get control() {
            if (this.htmlFor) return document.getElementById(this.htmlFor);
            return this.querySelector('button, input, meter, output, progress, select, textarea');
        }
        click() {
            super.click();
            this.control?.focus();
        }
    }
    class HTMLFieldSetElement extends HTMLElement {
        get elements() {
            return this.querySelectorAll('button, fieldset, input, object, output, select, textarea');
        }
        get form() { return associatedForm(this); }
        get disabled() { return this.hasAttribute('disabled'); }
        set disabled(value) { this.toggleAttribute('disabled', !!value); }
        get type() { return 'fieldset'; }
    }
    class HTMLDataListElement extends HTMLElement {
        get options() { return this.querySelectorAll('option'); }
    }
    class HTMLOutputElement extends HTMLElement {
        get htmlFor() { return this.__htmlFor ||= new DOMTokenList(this, 'for'); }
        get form() { return associatedForm(this); }
        get name() { return this.getAttribute('name') || ''; }
        set name(value) { this.setAttribute('name', value); }
        get type() { return 'output'; }
        get value() { return this.textContent; }
        set value(value) { this.textContent = String(value); }
        get defaultValue() { return this.__defaultValue ?? this.textContent; }
        set defaultValue(value) {
            value = String(value);
            if (this.__defaultValue === undefined) this.textContent = value;
            else this.__defaultValue = value;
        }
        get labels() { return labelsFor(this); }
        get willValidate() { return false; }
        get validity() { return validValidityState(); }
        get validationMessage() { return ''; }
        setCustomValidity(_message) {}
        checkValidity() { return true; }
        reportValidity() { return true; }
    }
    class HTMLProgressElement extends HTMLElement {
        get value() { return clampedNumberAttribute(this, 'value', 0, 0, this.max); }
        set value(value) { this.setAttribute('value', value); }
        get max() { return positiveNumberAttribute(this, 'max', 1); }
        set max(value) { this.setAttribute('max', value); }
        get position() { return this.hasAttribute('value') ? this.value / this.max : -1; }
        get labels() { return labelsFor(this); }
    }
    class HTMLMeterElement extends HTMLElement {
        get min() { return numberAttribute(this, 'min', 0); }
        set min(value) { this.setAttribute('min', value); }
        get max() { return Math.max(this.min, numberAttribute(this, 'max', 1)); }
        set max(value) { this.setAttribute('max', value); }
        get value() { return clampedNumberAttribute(this, 'value', 0, this.min, this.max); }
        set value(value) { this.setAttribute('value', value); }
        get low() { return clampedNumberAttribute(this, 'low', this.min, this.min, this.max); }
        set low(value) { this.setAttribute('low', value); }
        get high() { return clampedNumberAttribute(this, 'high', this.max, this.low, this.max); }
        set high(value) { this.setAttribute('high', value); }
        get optimum() { return clampedNumberAttribute(this, 'optimum', (this.min + this.max) / 2, this.min, this.max); }
        set optimum(value) { this.setAttribute('optimum', value); }
        get labels() { return labelsFor(this); }
    }
    class HTMLTemplateElement extends HTMLElement {
        get content() { return wrap(host('templateContent', this.__id)); }
    }
    class HTMLFormElement extends HTMLElement {
        get elements() {
            return document.querySelectorAll('button, fieldset, input, object, output, select, textarea')
                .filter(element => associatedForm(element) === this);
        }
        get length() { return this.elements.length; }
        get noValidate() { return this.hasAttribute('novalidate'); }
        set noValidate(value) { this.toggleAttribute('novalidate', !!value); }
        checkValidity() {
            let valid = true;
            for (const control of this.elements) if (typeof control.checkValidity === 'function' && !control.checkValidity()) valid = false;
            return valid;
        }
        reportValidity() { return this.checkValidity(); }
    }
    function reflectedInteger(element, attribute, fallback) {
        const value = Number(element.getAttribute(attribute));
        return Number.isFinite(value) ? Math.trunc(value) : fallback;
    }
    function numberAttribute(element, attribute, fallback) {
        const value = Number(element.getAttribute(attribute));
        return Number.isFinite(value) ? value : fallback;
    }
    function positiveNumberAttribute(element, attribute, fallback) {
        const value = numberAttribute(element, attribute, fallback);
        return value > 0 ? value : fallback;
    }
    function clampedNumberAttribute(element, attribute, fallback, minimum, maximum) {
        return Math.min(maximum, Math.max(minimum, numberAttribute(element, attribute, fallback)));
    }
    function labelsFor(element) {
        return document.querySelectorAll('label').filter(label => label.control === element);
    }
    function validValidityState() {
        return {
            valueMissing: false, typeMismatch: false, patternMismatch: false,
            tooLong: false, tooShort: false, rangeUnderflow: false,
            rangeOverflow: false, stepMismatch: false, badInput: false,
            customError: false, valid: true
        };
    }
    function validityFor(element) {
        const value = String(element.value ?? '');
        const type = String(element.type || '').toLowerCase();
        const required = !!element.required;
        const valueMissing = required && value === '';
        let typeMismatch = false;
        if (value && type === 'email') typeMismatch = !/^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(value);
        if (value && type === 'url') typeMismatch = !/^[a-z][a-z0-9+.-]*:\/\/[^\s]+$/i.test(value);
        let patternMismatch = false;
        const pattern = element.pattern;
        if (value && pattern) {
            try { patternMismatch = !(new RegExp('^(?:' + pattern + ')$')).test(value); } catch (_error) {}
        }
        const numeric = Number(value);
        const hasNumber = value !== '' && Number.isFinite(numeric);
        const minimum = Number(element.min);
        const maximum = Number(element.max);
        const rangeUnderflow = hasNumber && element.min !== '' && Number.isFinite(minimum) && numeric < minimum;
        const rangeOverflow = hasNumber && element.max !== '' && Number.isFinite(maximum) && numeric > maximum;
        const badInput = (type === 'number' || type === 'range') && value !== '' && !hasNumber;
        const customError = !!element.__customValidity;
        const valid = !(valueMissing || typeMismatch || patternMismatch || rangeUnderflow || rangeOverflow || badInput || customError);
        return {
            valueMissing, typeMismatch, patternMismatch,
            tooLong: false, tooShort: false, rangeUnderflow,
            rangeOverflow, stepMismatch: false, badInput,
            customError, valid
        };
    }
    function checkControlValidity(element) {
        if (!element.willValidate || element.validity.valid) return true;
        element.dispatchEvent(new Event('invalid', { cancelable: true }));
        return false;
    }
    function associatedForm(element) {
        const explicit = element.getAttribute('form');
        if (explicit) {
            const form = document.getElementById(explicit);
            return form?.localName === 'form' ? form : null;
        }
        for (let ancestor = element.parentElement; ancestor; ancestor = ancestor.parentElement) {
            if (ancestor.localName === 'form') return ancestor;
        }
        return null;
    }
    Object.defineProperties(HTMLElement.prototype, {
        onchange: { configurable: true, writable: true, value: null },
        onerror: { configurable: true, writable: true, value: null },
        oninput: { configurable: true, writable: true, value: null },
        oninvalid: { configurable: true, writable: true, value: null }
    });

    const htmlNamespace = 'http://www.w3.org/1999/xhtml';
    const knownHtmlElements = new Set((
        'html head title base link meta style body article section nav aside h1 h2 h3 h4 h5 h6 ' +
        'hgroup header footer address p hr pre blockquote ol ul menu li dl dt dd figure figcaption ' +
        'main search div a em strong small s cite q dfn abbr ruby rt rp data time code var samp kbd ' +
        'sub sup i b u mark bdi bdo span br wbr ins del picture source img iframe embed object video ' +
        'audio track map area table caption colgroup col tbody thead tfoot tr td th form label input ' +
        'button select datalist optgroup option textarea output progress meter fieldset legend details ' +
        'summary dialog script noscript template slot canvas acronym applet basefont bgsound big blink ' +
        'center content dir font frame frameset image keygen marquee menuitem nobr noembed noframes ' +
        'param plaintext rb rtc shadow spacer strike tt xmp'
    ).split(/\s+/));
    const htmlElementConstructor = localName => {
        if (localName === 'time') return HTMLTimeElement;
        if (localName === 'data') return HTMLDataElement;
        if (localName === 'a') return HTMLAnchorElement;
        if (localName === 'details') return HTMLDetailsElement;
        if (localName === 'dialog') return HTMLDialogElement;
        if (localName === 'script') return HTMLScriptElement;
        if (localName === 'img') return HTMLImageElement;
        if (localName === 'picture') return HTMLPictureElement;
        if (localName === 'source') return HTMLSourceElement;
        if (localName === 'input') return HTMLInputElement;
        if (localName === 'textarea') return HTMLTextAreaElement;
        if (localName === 'ol') return HTMLOrderedListElement;
        if (localName === 'select') return HTMLSelectElement;
        if (localName === 'button') return HTMLButtonElement;
        if (localName === 'label') return HTMLLabelElement;
        if (localName === 'fieldset') return HTMLFieldSetElement;
        if (localName === 'datalist') return HTMLDataListElement;
        if (localName === 'output') return HTMLOutputElement;
        if (localName === 'progress') return HTMLProgressElement;
        if (localName === 'meter') return HTMLMeterElement;
        if (localName === 'template') return HTMLTemplateElement;
        if (localName === 'form') return HTMLFormElement;
        return knownHtmlElements.has(localName) || localName.includes('-')
            ? HTMLElement
            : HTMLUnknownElement;
    };

    class Document extends Node {
        constructor(id) {
            super(id);
            this.readyState = 'loading';
            this.activeElement = null;
            this._currentScript = null;
        }
        createElement(name) { return wrap(host('createElement', String(name))); }
        createElementNS(_namespace, name) { return this.createElement(name); }
        createTextNode(text) { return wrap(host('createText', String(text))); }
        createComment(text) { return wrap(host('createComment', String(text))); }
        createEvent(type) {
            const event = new Event('');
            event.initEvent = function(name, bubbles = false, cancelable = false) {
                this.type = String(name); this.bubbles = !!bubbles; this.cancelable = !!cancelable;
            };
            event.initCustomEvent = function(name, bubbles = false, cancelable = false, detail = null) {
                this.initEvent(name, bubbles, cancelable); this.detail = detail;
            };
            return event;
        }
        getElementById(id) { return wrap(host('byId', String(id))); }
        getElementsByTagName(name) { return this.querySelectorAll(String(name)); }
        getElementsByClassName(name) { return this.querySelectorAll('.' + String(name).trim().replace(/\s+/g, '.')); }
        getElementsByName(name) { return this.querySelectorAll('[name="' + String(name).replace(/"/g, '\\"') + '"]'); }
        get documentElement() { return this.querySelector('html'); }
        get head() { return this.querySelector('head'); }
        get body() { return this.querySelector('body'); }
        get title() { return this.querySelector('title')?.textContent || ''; }
        set title(value) {
            let title = this.querySelector('title');
            if (!title) { title = this.createElement('title'); (this.head || this.documentElement).appendChild(title); }
            title.textContent = String(value);
        }
        get URL() { return host('documentUrl'); }
        get documentURI() { return this.URL; }
        get baseURI() { return this.querySelector('base')?.href || this.URL; }
        get currentScript() { return this._currentScript; }
        get defaultView() { return windowObject; }
        __setCurrentScript(id) { this._currentScript = wrap(id); }
        write(...parts) { host('innerHtmlAppend', (this.body || this.documentElement).__id, parts.join('')); }
        writeln(...parts) { this.write(parts.join('') + '\n'); }
        hasFocus() { return true; }
        get hidden() { return false; }
        get visibilityState() { return 'visible'; }
        get compatMode() { return 'CSS1Compat'; }
        get characterSet() { return 'UTF-8'; }
        get contentType() { return 'text/html'; }
        get cookie() { return host('cookieGet'); }
        set cookie(value) { host('cookieSet', String(value)); }
    }

    function wrap(id) {
        id = Number(id) || 0;
        if (!id) return null;
        if (cache.has(id)) return cache.get(id);
        const type = host('nodeType', id);
        let node;
        if (type === 9) node = new Document(id);
        else if (type === 1) {
            const namespace = host('namespaceUri', id);
            const Constructor = namespace === htmlNamespace
                ? htmlElementConstructor(host('localName', id))
                : Element;
            node = new Constructor(id);
        }
        else if (type === 11) node = new DocumentFragment(id);
        else node = type === 8 ? new Comment(id) : new Text(id);
        cache.set(id, node);
        return node;
    }

    const document = wrap(host('document'));
    const windowEvents = new EventTarget();
    const windowObject = globalThis;
    windowObject.window = windowObject;
    windowObject.self = windowObject;
    windowObject.top = windowObject;
    windowObject.parent = windowObject;
    windowObject.document = document;
    windowObject.Node = Node;
    windowObject.Element = Element;
    windowObject.HTMLElement = HTMLElement;
    windowObject.HTMLUnknownElement = HTMLUnknownElement;
    windowObject.HTMLTimeElement = HTMLTimeElement;
    windowObject.HTMLDataElement = HTMLDataElement;
    windowObject.HTMLAnchorElement = HTMLAnchorElement;
    windowObject.HTMLDetailsElement = HTMLDetailsElement;
    windowObject.HTMLDialogElement = HTMLDialogElement;
    windowObject.HTMLScriptElement = HTMLScriptElement;
    windowObject.HTMLImageElement = HTMLImageElement;
    windowObject.HTMLPictureElement = HTMLPictureElement;
    windowObject.HTMLSourceElement = HTMLSourceElement;
    windowObject.HTMLInputElement = HTMLInputElement;
    windowObject.HTMLTextAreaElement = HTMLTextAreaElement;
    windowObject.HTMLOrderedListElement = HTMLOrderedListElement;
    windowObject.HTMLSelectElement = HTMLSelectElement;
    windowObject.HTMLButtonElement = HTMLButtonElement;
    windowObject.HTMLLabelElement = HTMLLabelElement;
    windowObject.HTMLFieldSetElement = HTMLFieldSetElement;
    windowObject.HTMLDataListElement = HTMLDataListElement;
    windowObject.HTMLOutputElement = HTMLOutputElement;
    windowObject.HTMLProgressElement = HTMLProgressElement;
    windowObject.HTMLMeterElement = HTMLMeterElement;
    windowObject.HTMLTemplateElement = HTMLTemplateElement;
    windowObject.HTMLFormElement = HTMLFormElement;
    windowObject.Document = Document;
    windowObject.Text = Text;
    windowObject.DocumentFragment = DocumentFragment;
    windowObject.Event = Event;
    windowObject.CustomEvent = CustomEvent;
    windowObject.MessageEvent = MessageEvent;
    windowObject.ToggleEvent = ToggleEvent;
    windowObject.DOMException = DOMException;
    windowObject.EventTarget = EventTarget;
    windowObject.DOMTokenList = DOMTokenList;
    windowObject.DOMStringMap = DOMStringMap;
    windowObject.CSSStyleDeclaration = CSSStyleDeclaration;
    Object.defineProperty(windowEvents, '__eventTargetProxy', { value: windowObject });
    windowObject.addEventListener = windowEvents.addEventListener.bind(windowEvents);
    windowObject.removeEventListener = windowEvents.removeEventListener.bind(windowEvents);
    windowObject.dispatchEvent = windowEvents.dispatchEvent.bind(windowEvents);

    const iframeWindow = isolatedIframeWindow || windowObject;
    const iframeEvents = new EventTarget();
    Object.defineProperty(iframeEvents, '__eventTargetProxy', { value: iframeWindow });
    iframeWindow.addEventListener = iframeEvents.addEventListener.bind(iframeEvents);
    iframeWindow.removeEventListener = iframeEvents.removeEventListener.bind(iframeEvents);
    iframeWindow.dispatchEvent = iframeEvents.dispatchEvent.bind(iframeEvents);
    const iframeDocument = {
        defaultView: iframeWindow,
        readyState: 'complete',
        URL: 'about:blank',
        documentURI: 'about:blank',
        baseURI: 'about:blank',
        createElement: name => document.createElement(name),
        createElementNS: (_namespace, name) => document.createElement(name),
        createTextNode: text => document.createTextNode(text),
        querySelector: selector => document.querySelector(selector),
        querySelectorAll: selector => document.querySelectorAll(selector)
    };
    iframeWindow.parent = windowObject;
    iframeWindow.top = windowObject;
    iframeWindow.document = iframeDocument;

    let currentUrl = host('documentUrl');
    const parseUrl = value => {
        const match = String(value).match(/^([a-z]+:)?\/\/([^/?#]+)?([^?#]*)?(\?[^#]*)?(#.*)?$/i);
        return {
            protocol: match?.[1] || '',
            host: match?.[2] || '',
            hostname: (match?.[2] || '').split(':')[0],
            pathname: match?.[3] || '/',
            search: match?.[4] || '',
            hash: match?.[5] || ''
        };
    };
    const location = {
        get href() { return currentUrl; },
        set href(value) { currentUrl = host('navigate', String(value)); },
        assign(value) { this.href = value; },
        replace(value) { this.href = value; },
        reload() { host('navigate', currentUrl); },
        toString() { return currentUrl; },
        get protocol() { return parseUrl(currentUrl).protocol; },
        get host() { return parseUrl(currentUrl).host; },
        get hostname() { return parseUrl(currentUrl).hostname; },
        get pathname() { return parseUrl(currentUrl).pathname; },
        get search() { return parseUrl(currentUrl).search; },
        get hash() { return parseUrl(currentUrl).hash; },
        get origin() { const parsed = parseUrl(currentUrl); return parsed.protocol + '//' + parsed.host; }
    };
    windowObject.location = location;
    document.location = location;
    function cloneMessageValue(value, memory = new Map()) {
        if (value === null || ['undefined', 'boolean', 'number', 'string', 'bigint'].includes(typeof value)) return value;
        if (typeof value === 'symbol' || typeof value === 'function') {
            throw new DOMException('The value could not be cloned', 'DataCloneError');
        }
        if (memory.has(value)) return memory.get(value);
        if (typeof ArrayBuffer !== 'undefined' && value instanceof ArrayBuffer) return value.slice(0);
        if (typeof ArrayBuffer !== 'undefined' && ArrayBuffer.isView?.(value)) {
            const buffer = value.buffer.slice(value.byteOffset, value.byteOffset + value.byteLength);
            return typeof DataView !== 'undefined' && value instanceof DataView
                ? new DataView(buffer)
                : new value.constructor(buffer);
        }
        if (value instanceof Date) return new Date(value.getTime());
        if (value instanceof RegExp) return new RegExp(value.source, value.flags);
        if (value instanceof Map) {
            const clone = new Map();
            memory.set(value, clone);
            for (const [key, entry] of value) clone.set(cloneMessageValue(key, memory), cloneMessageValue(entry, memory));
            return clone;
        }
        if (value instanceof Set) {
            const clone = new Set();
            memory.set(value, clone);
            for (const entry of value) clone.add(cloneMessageValue(entry, memory));
            return clone;
        }
        const prototype = Object.getPrototypeOf(value);
        if (prototype !== Object.prototype && prototype !== null && !Array.isArray(value)) {
            throw new DOMException('The value could not be cloned', 'DataCloneError');
        }
        const clone = Array.isArray(value) ? [] : {};
        memory.set(value, clone);
        for (const key of Object.keys(value)) clone[key] = cloneMessageValue(value[key], memory);
        return clone;
    }
    const targetOriginValue = targetOrigin => {
        targetOrigin = targetOrigin === undefined ? '/' : String(targetOrigin);
        if (targetOrigin === '*' || targetOrigin === '/') return targetOrigin;
        const parsed = parseUrl(host('resolveUrl', targetOrigin));
        if (!parsed.protocol || !parsed.host) throw new DOMException('Invalid target origin', 'SyntaxError');
        return parsed.protocol + '//' + parsed.host;
    };
    function postMessageTo(targetEvents, message, targetOriginOrOptions = '/', transfer = []) {
        let targetOrigin = targetOriginOrOptions;
        if (targetOriginOrOptions && typeof targetOriginOrOptions === 'object') {
            targetOrigin = targetOriginOrOptions.targetOrigin ?? '/';
            transfer = targetOriginOrOptions.transfer || [];
        }
        if (transfer && transfer.length) {
            throw new DOMException('Transferable objects are not implemented', 'DataCloneError');
        }
        const cloned = cloneMessageValue(message);
        const expectedOrigin = targetOriginValue(targetOrigin);
        if (expectedOrigin !== '*' && expectedOrigin !== '/' && expectedOrigin !== location.origin) return;
        setTimeout(() => targetEvents.dispatchEvent(new MessageEvent('message', {
            data: cloned,
            origin: location.origin,
            source: windowObject,
            ports: []
        })), 0);
    }
    windowObject.postMessage = (message, targetOriginOrOptions = '/', transfer = []) =>
        postMessageTo(windowEvents, message, targetOriginOrOptions, transfer);
    iframeWindow.postMessage = (message, targetOriginOrOptions = '/', transfer = []) =>
        postMessageTo(iframeEvents, message, targetOriginOrOptions, transfer);
    windowObject.history = {
        length: 1,
        state: null,
        pushState(state, _title, url) { this.state = state; if (url != null) currentUrl = host('resolveUrl', String(url)); },
        replaceState(state, _title, url) { this.state = state; if (url != null) currentUrl = host('resolveUrl', String(url)); },
        back() {}, forward() {}, go() {}
    };

    const storage = () => {
        const values = new Map();
        return {
            get length() { return values.size; },
            key(index) { return [...values.keys()][index] || null; },
            getItem(key) { key = String(key); return values.has(key) ? values.get(key) : null; },
            setItem(key, value) { values.set(String(key), String(value)); },
            removeItem(key) { values.delete(String(key)); },
            clear() { values.clear(); }
        };
    };
    windowObject.localStorage = storage();
    windowObject.sessionStorage = storage();
    windowObject.navigator = {
        userAgent: host('userAgent'),
        appName: 'Netscape',
        appVersion: '5.0',
        platform: 'Win32',
        language: 'en-CA',
        languages: ['en-CA', 'en'],
        onLine: true,
        cookieEnabled: true,
        hardwareConcurrency: 1,
        maxTouchPoints: 0,
        sendBeacon(url) { host('console', 'beacon', String(url)); return false; },
        javaEnabled() { return false; }
    };
    iframeWindow.navigator = windowObject.navigator;
    windowObject.screen = { width: 1280, height: 720, availWidth: 1280, availHeight: 680, colorDepth: 24, pixelDepth: 24 };
    windowObject.innerWidth = 1280;
    windowObject.innerHeight = 720;
    windowObject.devicePixelRatio = 1;
    windowObject.scrollX = windowObject.pageXOffset = 0;
    windowObject.scrollY = windowObject.pageYOffset = 0;
    windowObject.scrollTo = windowObject.scrollBy = () => {};

    const started = Date.now();
    windowObject.performance = {
        timeOrigin: started,
        now() { return Date.now() - started; },
        mark() {}, measure() {}, getEntriesByType() { return []; },
        timing: { navigationStart: started }
    };
    const base64Alphabet = 'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/';
    windowObject.atob = value => {
        const input = String(value).replace(/[\t\n\f\r ]/g, '').replace(/=+$/, '');
        if (input.length % 4 === 1 || /[^A-Za-z0-9+/]/.test(input)) throw new Error('InvalidCharacterError');
        let bits = 0, bitCount = 0, output = '';
        for (const character of input) {
            bits = (bits << 6) | base64Alphabet.indexOf(character);
            bitCount += 6;
            if (bitCount >= 8) {
                bitCount -= 8;
                output += String.fromCharCode((bits >> bitCount) & 255);
            }
        }
        return output;
    };
    windowObject.btoa = value => {
        const input = String(value);
        let output = '', buffer = 0, bitCount = 0;
        for (let index = 0; index < input.length; index++) {
            const code = input.charCodeAt(index);
            if (code > 255) throw new Error('InvalidCharacterError');
            buffer = (buffer << 8) | code;
            bitCount += 8;
            while (bitCount >= 6) {
                bitCount -= 6;
                output += base64Alphabet[(buffer >> bitCount) & 63];
            }
        }
        if (bitCount > 0) output += base64Alphabet[(buffer << (6 - bitCount)) & 63];
        while (output.length % 4) output += '=';
        return output;
    };
    const unicodeScalarAt = (input, index) => {
        const first = input.charCodeAt(index);
        if (first >= 0xD800 && first <= 0xDBFF && index + 1 < input.length) {
            const second = input.charCodeAt(index + 1);
            if (second >= 0xDC00 && second <= 0xDFFF) {
                return [0x10000 + ((first - 0xD800) << 10) + second - 0xDC00, 2];
            }
        }
        return [first >= 0xD800 && first <= 0xDFFF ? 0xFFFD : first, 1];
    };
    const utf8Bytes = scalar => {
        if (scalar <= 0x7F) return [scalar];
        if (scalar <= 0x7FF) return [0xC0 | (scalar >> 6), 0x80 | (scalar & 0x3F)];
        if (scalar <= 0xFFFF) {
            return [0xE0 | (scalar >> 12), 0x80 | ((scalar >> 6) & 0x3F), 0x80 | (scalar & 0x3F)];
        }
        return [
            0xF0 | (scalar >> 18),
            0x80 | ((scalar >> 12) & 0x3F),
            0x80 | ((scalar >> 6) & 0x3F),
            0x80 | (scalar & 0x3F)
        ];
    };
    class TextEncoder {
        get encoding() { return 'utf-8'; }
        encode(input = '') {
            input = String(input);
            const output = [];
            for (let index = 0; index < input.length;) {
                const [scalar, units] = unicodeScalarAt(input, index);
                output.push(...utf8Bytes(scalar));
                index += units;
            }
            return new Uint8Array(output);
        }
        encodeInto(source, destination) {
            source = String(source);
            if (!(destination instanceof Uint8Array)) throw new TypeError('destination must be a Uint8Array');
            let read = 0;
            let written = 0;
            while (read < source.length) {
                const [scalar, units] = unicodeScalarAt(source, read);
                const bytes = utf8Bytes(scalar);
                if (written + bytes.length > destination.length) break;
                destination.set(bytes, written);
                written += bytes.length;
                read += units;
            }
            return { read, written };
        }
    }
    const decoderInputBytes = input => {
        if (input === undefined) return [];
        if (input instanceof ArrayBuffer) return [...new Uint8Array(input)];
        if (ArrayBuffer.isView?.(input)) return [...new Uint8Array(input.buffer, input.byteOffset, input.byteLength)];
        throw new TypeError('input must be an ArrayBuffer or an ArrayBuffer view');
    };
    const scalarString = scalar => scalar <= 0xFFFF
        ? String.fromCharCode(scalar)
        : String.fromCharCode(0xD800 + ((scalar - 0x10000) >> 10), 0xDC00 + ((scalar - 0x10000) & 0x3FF));
    class TextDecoder {
        constructor(label = 'utf-8', options = {}) {
            label = String(label).trim().toLowerCase();
            if (!['utf-8', 'utf8', 'unicode-1-1-utf-8'].includes(label)) {
                throw new RangeError('Only UTF-8 decoding is implemented');
            }
            this.__fatal = !!options.fatal;
            this.__ignoreBOM = !!options.ignoreBOM;
            this.__pending = [];
            this.__streaming = false;
            this.__bomSeen = false;
        }
        get encoding() { return 'utf-8'; }
        get fatal() { return this.__fatal; }
        get ignoreBOM() { return this.__ignoreBOM; }
        decode(input, options = {}) {
            const stream = !!options.stream;
            const bytes = (this.__streaming ? this.__pending : []).concat(decoderInputBytes(input));
            this.__pending = [];
            let output = '';
            let index = 0;
            const emit = scalar => {
                if (!this.__bomSeen) {
                    this.__bomSeen = true;
                    if (!this.__ignoreBOM && scalar === 0xFEFF) return;
                }
                output += scalarString(scalar);
            };
            const fail = () => {
                if (this.__fatal) throw new TypeError('The encoded data was not valid UTF-8');
                emit(0xFFFD);
            };
            while (index < bytes.length) {
                const first = bytes[index];
                if (first <= 0x7F) {
                    emit(first);
                    index++;
                    continue;
                }
                let needed = 0;
                let scalar = 0;
                let minimum = 0;
                if (first >= 0xC2 && first <= 0xDF) {
                    needed = 1; scalar = first & 0x1F; minimum = 0x80;
                } else if (first >= 0xE0 && first <= 0xEF) {
                    needed = 2; scalar = first & 0x0F; minimum = 0x800;
                } else if (first >= 0xF0 && first <= 0xF4) {
                    needed = 3; scalar = first & 0x07; minimum = 0x10000;
                } else {
                    fail();
                    index++;
                    continue;
                }
                if (index + needed >= bytes.length) {
                    if (stream) this.__pending = bytes.slice(index);
                    else fail();
                    index = bytes.length;
                    break;
                }
                let valid = true;
                for (let offset = 1; offset <= needed; offset++) {
                    const continuation = bytes[index + offset];
                    if ((continuation & 0xC0) !== 0x80) { valid = false; break; }
                    scalar = (scalar << 6) | (continuation & 0x3F);
                }
                if (!valid || scalar < minimum || scalar > 0x10FFFF || (scalar >= 0xD800 && scalar <= 0xDFFF)) {
                    fail();
                    index++;
                    continue;
                }
                emit(scalar);
                index += needed + 1;
            }
            this.__streaming = stream;
            if (!stream) {
                this.__pending = [];
                this.__bomSeen = false;
            }
            return output;
        }
    }
    windowObject.TextEncoder = TextEncoder;
    windowObject.TextDecoder = TextDecoder;
    const makeConsole = level => (...args) => host('console', level, args.map(value => {
        try { return typeof value === 'string' ? value : JSON.stringify(value); }
        catch (_) { return String(value); }
    }).join(' '));
    windowObject.console = {
        log: makeConsole('log'), info: makeConsole('info'), warn: makeConsole('warn'),
        error: makeConsole('error'), debug: makeConsole('debug'), trace: makeConsole('trace'),
        assert(condition, ...args) { if (!condition) makeConsole('assert')(...args); },
        time() {}, timeEnd() {}, group() {}, groupEnd() {}
    };

    let nextTimer = 1;
    const timers = new Map();
    const queueTimer = (callback, delay, repeat, args) => {
        const id = nextTimer++;
        delay = Math.max(0, Number(delay) || 0);
        timers.set(id, { callback, repeat, args });
        host('timerSchedule', id, delay, repeat);
        return id;
    };
    windowObject.setTimeout = (callback, delay, ...args) => queueTimer(callback, delay, false, args);
    windowObject.setInterval = (callback, delay, ...args) => queueTimer(callback, delay, true, args);
    windowObject.clearTimeout = windowObject.clearInterval = id => {
        id = Number(id);
        timers.delete(id);
        host('timerCancel', id);
    };
    windowObject.requestAnimationFrame = callback => queueTimer(() => callback(performance.now()), 16, false, []);
    windowObject.cancelAnimationFrame = windowObject.clearTimeout;
    windowObject.queueMicrotask = callback => Promise.resolve().then(callback);
    windowObject.__runTimer = id => {
        const timer = timers.get(Number(id));
        if (!timer) return false;
        if (!timer.repeat) timers.delete(Number(id));
        if (typeof timer.callback === 'function') timer.callback(...timer.args);
        else (0, eval)(String(timer.callback));
        return true;
    };

    class URLSearchParams {
        constructor(init = '') {
            this.values = [];
            const source = String(init).replace(/^\?/, '');
            if (source) for (const part of source.split('&')) {
                const [key, value = ''] = part.split('=');
                this.values.push([decodeURIComponent(key.replace(/\+/g, ' ')), decodeURIComponent(value.replace(/\+/g, ' '))]);
            }
        }
        append(key, value) { this.values.push([String(key), String(value)]); }
        set(key, value) { this.delete(key); this.append(key, value); }
        get(key) { return this.values.find(entry => entry[0] === String(key))?.[1] ?? null; }
        getAll(key) { return this.values.filter(entry => entry[0] === String(key)).map(entry => entry[1]); }
        has(key) { return this.values.some(entry => entry[0] === String(key)); }
        delete(key) { key = String(key); this.values = this.values.filter(entry => entry[0] !== key); }
        toString() { return this.values.map(([key, value]) => encodeURIComponent(key) + '=' + encodeURIComponent(value)).join('&'); }
        entries() { return this.values[Symbol.iterator](); }
        keys() { return this.values.map(entry => entry[0])[Symbol.iterator](); }
        values() { return this.values.map(entry => entry[1])[Symbol.iterator](); }
        [Symbol.iterator]() { return this.entries(); }
    }
    windowObject.URLSearchParams = URLSearchParams;
    windowObject.URL = class URL {
        constructor(value, base = currentUrl) { this.href = host('resolveUrl', String(value || base)); }
        toString() { return this.href; }
        toJSON() { return this.href; }
        get protocol() { return parseUrl(this.href).protocol; }
        get host() { return parseUrl(this.href).host; }
        get hostname() { return parseUrl(this.href).hostname; }
        get pathname() { return parseUrl(this.href).pathname; }
        get search() { return parseUrl(this.href).search; }
        get hash() { return parseUrl(this.href).hash; }
        get origin() { const parsed = parseUrl(this.href); return parsed.protocol + '//' + parsed.host; }
        get searchParams() { return new URLSearchParams(this.search); }
    };

    const computedStyleProxy = element => new Proxy({
        getPropertyValue(name) {
            name = String(name).toLowerCase();
            const inline = element?.style?.getPropertyValue(name) || '';
            return inline || (element ? host('uaStyle', element.__id, name) : '');
        },
        get cssText() { return ''; }
    }, {
        get(target, property) {
            if (property in target) {
                const value = target[property];
                return typeof value === 'function' ? value.bind(target) : value;
            }
            return target.getPropertyValue(String(property).replace(/[A-Z]/g, match => '-' + match.toLowerCase()));
        }
    });
    windowObject.getComputedStyle = element => computedStyleProxy(element);
    windowObject.matchMedia = query => ({ media: String(query), matches: false, onchange: null, addListener() {}, removeListener() {}, addEventListener() {}, removeEventListener() {}, dispatchEvent() { return true; } });
    windowObject.CSS = { supports() { return false; }, escape(value) { return String(value).replace(/[^a-zA-Z0-9_-]/g, match => '\\' + match); } };
    windowObject.Image = class Image extends HTMLImageElement {
        constructor() {
            const element = document.createElement('img');
            Object.defineProperty(element, 'src', {
                configurable: true,
                get() { const value = this.getAttribute('src'); return value == null ? '' : host('resolveUrl', value); },
                set(value) {
                    this.setAttribute('src', String(value));
                    setTimeout(() => this.dispatchEvent(new Event('error')), 0);
                }
            });
            return element;
        }
    };
    windowObject.MutationObserver = class { constructor(callback) { this.callback = callback; } observe() {} disconnect() {} takeRecords() { return []; } };
    windowObject.IntersectionObserver = class { constructor(callback) { this.callback = callback; } observe() {} unobserve() {} disconnect() {} takeRecords() { return []; } };
    windowObject.ResizeObserver = class { constructor(callback) { this.callback = callback; } observe() {} unobserve() {} disconnect() {} };
    windowObject.fetch = () => Promise.reject(new TypeError('fetch is not implemented yet'));
    windowObject.XMLHttpRequest = class XMLHttpRequest extends EventTarget {
        constructor() { super(); this.readyState = 0; this.status = 0; this.responseText = ''; }
        open(method, url) { this.method = method; this.url = host('resolveUrl', String(url)); this.readyState = 1; }
        setRequestHeader() {}
        send() { this.readyState = 4; this.dispatchEvent(new Event('error')); this.dispatchEvent(new Event('readystatechange')); }
        abort() {}
    };
    windowObject.crypto = {
        getRandomValues(array) {
            for (let index = 0; index < array.length; index++) array[index] = Math.floor(Math.random() * 256);
            return array;
        },
        randomUUID() {
            return 'xxxxxxxx-xxxx-4xxx-yxxx-xxxxxxxxxxxx'.replace(/[xy]/g, character => {
                const value = Math.floor(Math.random() * 16);
                return (character === 'x' ? value : (value & 3) | 8).toString(16);
            });
        }
    };

    windowObject.__wrap = wrap;
    windowObject.__finishDocument = () => {
        document.readyState = 'interactive';
        document.dispatchEvent(new Event('DOMContentLoaded'));
        document.readyState = 'complete';
        windowObject.dispatchEvent(new Event('load'));
    };
})();
"#;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::dom;

    fn execute_html(html: &str) -> (super::super::dom::Dom, ScriptOutcome) {
        let dom = dom::parse_with_scripting(html, true);
        let scripts = dom
            .elements_named("script")
            .map(|node| ScriptInput {
                source_url: "https://example.com/#inline".into(),
                code: node.text_content(),
                node,
                finish_lifecycle: true,
            })
            .collect::<Vec<_>>();
        let outcome = execute(dom.document.clone(), "https://example.com/", &scripts);
        (dom, outcome)
    }

    #[test]
    fn executes_script_and_mutates_the_owned_dom() {
        let (dom, outcome) = execute_html(
            r#"<body><main id="app"></main><script>
                const message = document.createElement('p');
                message.className = 'result';
                message.textContent = 'JavaScript works';
                document.getElementById('app').appendChild(message);
            </script></body>"#,
        );
        assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
        assert_eq!(outcome.executed, 1);
        let paragraph = dom.elements_named("p").next().unwrap();
        assert_eq!(paragraph.attr("class").as_deref(), Some("result"));
        assert_eq!(paragraph.text_content(), "JavaScript works");
    }

    #[test]
    fn executes_classic_scripts_with_html_like_comments() {
        let (dom, outcome) = execute_html(
            r#"<body><div id="status">waiting</div><script>
                <!--
                document.getElementById('status').textContent = 'ready';
                -->
            </script></body>"#,
        );
        assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
        assert_eq!(outcome.executed, 1);
        assert_eq!(
            dom.elements_named("div").next().unwrap().text_content(),
            "ready"
        );
    }

    #[test]
    fn loads_dynamically_inserted_external_scripts_in_the_same_realm() {
        let dom = dom::parse_with_scripting(
            r#"<html><head></head><body><div id="status">waiting</div><script>
                window.initialValue = 40;
                const loader = document.createElement('script');
                loader.src = '/dynamic.js';
                loader.onload = () => {
                    document.getElementById('status').textContent = String(window.dynamicAnswer);
                };
                document.head.appendChild(loader);
            </script></body></html>"#,
            true,
        );
        let scripts = dom
            .elements_named("script")
            .map(|node| ScriptInput {
                source_url: "https://example.com/#inline".into(),
                code: node.text_content(),
                node,
                finish_lifecycle: true,
            })
            .collect::<Vec<_>>();
        let mut requested = Vec::new();
        let mut loader = |url: &str| {
            requested.push(url.to_string());
            Ok("window.dynamicAnswer = window.initialValue + 2;".to_string())
        };
        let outcome = execute_with_loader(
            dom.document.clone(),
            "https://example.com/",
            &scripts,
            &mut loader,
        );

        assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
        assert_eq!(outcome.executed, 2);
        assert_eq!(requested, ["https://example.com/dynamic.js"]);
        assert_eq!(
            dom.elements_named("div").next().unwrap().text_content(),
            "42"
        );
    }

    #[test]
    fn image_constructor_reports_failed_load_asynchronously() {
        let (dom, outcome) = execute_html(
            r#"<body><div id="status">waiting</div><script>
                const image = new Image();
                image.src = 'data:image/unsupported;base64,AAAA';
                image.onerror = () => {
                    document.getElementById('status').textContent = 'unsupported';
                };
            </script></body>"#,
        );
        assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
        assert_eq!(
            dom.elements_named("div").next().unwrap().text_content(),
            "unsupported"
        );
    }

    #[test]
    fn exposes_html_element_identity_namespaces_and_ua_defaults() {
        let (dom, outcome) = execute_html(
            r##"<body><div id="status">no</div><script>
                const container = document.createElement('div');
                container.innerHTML = '<svg><circle></circle></svg><math><mi>x</mi></math>';
                const section = document.createElement('section');
                const unknown = document.createElement('madeupelement');
                const time = document.createElement('time');
                const data = document.createElement('data');
                const image = document.createElement('img');
                const picture = document.createElement('picture');
                const source = document.createElement('source');
                const input = document.createElement('input');
                const mark = document.createElement('mark');
                const rp = document.createElement('rp');
                const parent = document.createElement('div');
                const translatedChild = document.createElement('span');
                const list = document.createElement('ol');
                const select = document.createElement('select');
                const fieldset = document.createElement('fieldset');
                const field = document.createElement('input');
                const form = document.createElement('form');
                const externalField = document.createElement('input');
                const label = document.createElement('label');
                parent.translate = false;
                parent.appendChild(translatedChild);
                parent.accessKey = 'x';
                list.reversed = true;
                fieldset.appendChild(field);
                form.id = 'owner';
                externalField.id = 'owned-field';
                externalField.setAttribute('form', 'owner');
                label.htmlFor = 'owned-field';
                document.body.appendChild(form);
                document.body.appendChild(externalField);
                document.body.appendChild(label);
                time.dateTime = '2026-08-13';
                data.value = '42';
                image.srcset = 'small.png 1x, large.png 2x';
                image.sizes = '100vw';
                source.srcset = 'wide.png 2x';
                source.sizes = '50vw';
                source.media = '(min-width: 600px)';
                input.placeholder = 'Search';
                if (
                    section instanceof HTMLElement &&
                    !(section instanceof HTMLUnknownElement) &&
                    unknown instanceof HTMLUnknownElement &&
                    time instanceof HTMLTimeElement && time.getAttribute('datetime') === '2026-08-13' &&
                    data instanceof HTMLDataElement && data.getAttribute('value') === '42' &&
                    image instanceof HTMLImageElement && image.getAttribute('srcset').includes('large.png') &&
                    image.sizes === '100vw' && picture instanceof HTMLPictureElement &&
                    source instanceof HTMLSourceElement && source.srcset === 'wide.png 2x' &&
                    source.sizes === '50vw' && source.media === '(min-width: 600px)' &&
                    input instanceof HTMLInputElement && input.getAttribute('placeholder') === 'Search' &&
                    'onerror' in image &&
                    getComputedStyle(section).display === 'block' &&
                    getComputedStyle(mark).backgroundColor === 'rgb(255, 255, 0)' &&
                    getComputedStyle(rp).display === 'none' &&
                    translatedChild.translate === false &&
                    parent.getAttribute('translate') === 'no' && parent.accessKey === 'x' &&
                    typeof parent.accessKeyLabel === 'string' &&
                    list instanceof HTMLOrderedListElement && list.hasAttribute('reversed') &&
                    select instanceof HTMLSelectElement &&
                    fieldset instanceof HTMLFieldSetElement && fieldset.elements[0] === field &&
                    externalField.form === form && label instanceof HTMLLabelElement &&
                    label.control === externalField &&
                    container.firstChild.namespaceURI === 'http://www.w3.org/2000/svg' &&
                    container.lastChild.namespaceURI === 'http://www.w3.org/1998/Math/MathML'
                ) document.getElementById('status').textContent = 'yes';
            </script></body>"##,
        );
        assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
        assert_eq!(
            dom.elements_named("div").next().unwrap().text_content(),
            "yes"
        );
    }

    #[test]
    fn exposes_dataset_and_core_html_form_interfaces() {
        let (dom, outcome) = execute_html(
            r##"<body><div id="status">no</div><script>
                const data = document.createElement('div');
                data.setAttribute('data-user-id', '41');
                const sameDataset = data.dataset === data.dataset;
                data.dataset.userId = '42';
                data.dataset.displayName = 'Ada';
                const datasetKeys = Object.keys(data.dataset).sort().join(',');
                delete data.dataset.displayName;

                const form = document.createElement('form');
                form.id = 'owner';
                const input = document.createElement('input');
                input.id = 'email';
                input.type = 'email';
                input.required = true;
                input.value = 'not-an-email';
                input.selectionDirection = 'backward';
                input.formAction = '/submit';
                input.formMethod = 'post';
                input.formNoValidate = true;
                let inputEvents = 0;
                let invalidEvents = 0;
                input.oninput = () => inputEvents++;
                input.onchange = () => inputEvents++;
                input.oninvalid = () => invalidEvents++;
                input.dispatchEvent(new Event('input'));
                input.dispatchEvent(new Event('change'));
                form.appendChild(input);
                document.body.appendChild(form);
                const label = document.createElement('label');
                label.htmlFor = 'email';
                document.body.appendChild(label);

                const datalist = document.createElement('datalist');
                datalist.id = 'choices';
                datalist.appendChild(document.createElement('option'));
                input.setAttribute('list', 'choices');
                document.body.appendChild(datalist);

                const textarea = document.createElement('textarea');
                textarea.minLength = 2;
                textarea.maxLength = 20;
                textarea.wrap = 'hard';
                const select = document.createElement('select');
                select.required = true;
                const fieldset = document.createElement('fieldset');
                fieldset.disabled = true;
                const output = document.createElement('output');
                output.value = 'ready';
                const progress = document.createElement('progress');
                progress.max = 10;
                progress.value = 4;
                const meter = document.createElement('meter');
                meter.min = 0;
                meter.max = 100;
                meter.value = 75;
                const formIsValid = form.checkValidity();

                if (
                    data.dataset instanceof DOMStringMap && sameDataset && data.dataset.userId === '42' &&
                    data.getAttribute('data-user-id') === '42' && !data.hasAttribute('data-display-name') &&
                    datasetKeys === 'displayName,userId' && input instanceof HTMLInputElement &&
                    input.selectionDirection === 'backward' && !input.validity.valid &&
                    input.form === form && input.labels[0] === label && form.elements[0] === input &&
                    input.formAction === 'https://example.com/submit' && input.formMethod === 'post' &&
                    input.formNoValidate && datalist instanceof HTMLDataListElement &&
                    input.list === datalist && datalist.options.length === 1 &&
                    textarea instanceof HTMLTextAreaElement && textarea.minLength === 2 &&
                    textarea.maxLength === 20 && textarea.wrap === 'hard' &&
                    select instanceof HTMLSelectElement && select.required &&
                    fieldset instanceof HTMLFieldSetElement && fieldset.disabled &&
                    output instanceof HTMLOutputElement && output.value === 'ready' &&
                    progress instanceof HTMLProgressElement && progress.position === 0.4 &&
                    meter instanceof HTMLMeterElement && meter.value === 75 &&
                    form instanceof HTMLFormElement && !formIsValid &&
                    inputEvents === 2 && invalidEvents === 1
                ) document.getElementById('status').textContent = 'yes';
            </script></body>"##,
        );
        assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
        assert_eq!(
            dom.elements_named("div").next().unwrap().text_content(),
            "yes"
        );
    }

    #[test]
    fn exposes_template_contents_as_a_document_fragment() {
        let (dom, outcome) = execute_html(
            r#"<body><template id="parsed"><span id="inside">parsed</span></template><div id="status">no</div><script>
                const parsed = document.getElementById('parsed');
                const created = document.createElement('template');
                created.innerHTML = '<p data-value="42">created</p>';
                const paragraph = created.content.querySelector('p');
                if (
                    parsed instanceof HTMLTemplateElement &&
                    parsed.firstChild === null &&
                    parsed.content instanceof DocumentFragment &&
                    parsed.content.nodeType === 11 &&
                    parsed.content.ownerDocument === document &&
                    parsed.content.firstChild.textContent === 'parsed' &&
                    document.getElementById('inside') === null &&
                    paragraph.textContent === 'created' &&
                    paragraph.dataset.value === '42' &&
                    created.innerHTML.includes('<p data-value="42">created</p>') &&
                    !created.content.isConnected
                ) document.getElementById('status').textContent = 'yes';
            </script></body>"#,
        );
        assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
        assert_eq!(
            dom.elements_named("div").next().unwrap().text_content(),
            "yes"
        );
    }

    #[test]
    fn exposes_links_interactive_elements_and_script_reflection() {
        let (dom, outcome) = execute_html(
            r#"<body><div id="status">no</div><script>
                const anchor = document.createElement('a');
                anchor.download = 'report.txt';
                anchor.ping = '/audit-one /audit-two';
                anchor.relList.add('noopener', 'noreferrer');

                const details = document.createElement('details');
                const summary = document.createElement('summary');
                summary.textContent = 'More';
                const content = document.createElement('p');
                content.textContent = 'Details';
                details.append(summary, content);
                document.body.appendChild(details);
                const closedDisplay = getComputedStyle(content).display;
                summary.click();
                const openDisplay = getComputedStyle(content).display;

                const dialog = document.createElement('dialog');
                document.body.appendChild(dialog);
                const closedDialogDisplay = getComputedStyle(dialog).display;
                let closeEvents = 0;
                dialog.addEventListener('close', () => closeEvents++);
                dialog.showModal();
                const openDialogDisplay = getComputedStyle(dialog).display;
                dialog.close('accepted');

                const reflectedScript = document.createElement('script');
                reflectedScript.async = true;
                reflectedScript.defer = true;
                reflectedScript.text = 'window.answer = 42';
                setTimeout(() => {
                    if (
                        anchor instanceof HTMLAnchorElement && anchor.download === 'report.txt' &&
                        anchor.ping.includes('/audit-two') && anchor.relList.contains('noopener') &&
                        details instanceof HTMLDetailsElement && details.open &&
                        closedDisplay === 'none' && openDisplay === 'block' &&
                        dialog instanceof HTMLDialogElement && !dialog.open && dialog.returnValue === 'accepted' &&
                        closedDialogDisplay === 'none' && openDialogDisplay === 'block' && closeEvents === 1 &&
                        reflectedScript instanceof HTMLScriptElement && reflectedScript.async && reflectedScript.defer &&
                        reflectedScript.text.includes('answer')
                    ) document.getElementById('status').textContent = 'yes';
                }, 0);
            </script></body>"#,
        );
        assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
        assert_eq!(
            dom.elements_named("div").next().unwrap().text_content(),
            "yes"
        );
    }

    #[test]
    fn encodes_utf8_and_delivers_cloned_window_messages() {
        let (dom, outcome) = execute_html(
            r#"<body><div id="status">no</div><script>
                const bytes = new TextEncoder().encode('A¢😀');
                const decoded = new TextDecoder().decode(bytes);
                const destination = new Uint8Array(4);
                const progress = new TextEncoder().encodeInto('¢BC', destination);
                let messages = 0;
                window.addEventListener('message', event => {
                    messages++;
                    if (
                        decoded === 'A¢😀' && bytes.join(',') === '65,194,162,240,159,152,128' &&
                        progress.read === 3 && progress.written === 4 &&
                        event.origin === location.origin && event.source === window &&
                        event.data.nested.value === 42 && event.data !== payload
                    ) document.getElementById('status').textContent = 'yes';
                });
                const payload = { nested: { value: 42 } };
                window.postMessage(payload, location.origin);
                payload.nested.value = 7;
                window.postMessage('discarded', 'https://other.example');
            </script></body>"#,
        );
        assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
        assert_eq!(
            dom.elements_named("div").next().unwrap().text_content(),
            "yes"
        );
    }

    #[test]
    fn exposes_tokenizer_results_through_dom_bindings() {
        let (dom, outcome) = execute_html(
            r##"<body><div id="status">no</div><script>
                let result = true;
                const failures = [];
                const check = (name, value) => { if (!value) failures.push(name); return value; };
                const e = document.createElement('div');
                e.innerHTML = '<div<div>';
                result &= check('tag-name', e.firstChild && e.firstChild.nodeName === 'DIV<DIV');
                e.innerHTML = "<div foo<bar=''>";
                result &= check('attribute-name', e.firstChild.attributes[0].name === 'foo<bar');
                e.innerHTML = '<div foo=`bar`>';
                result &= check('unquoted-attribute', e.firstChild.getAttribute('foo') === '`bar`');
                e.innerHTML = "<div \"foo=''>";
                result &= check('quoted-name', e.firstChild.attributes[0].name === '"foo');
                e.innerHTML = "<a href='\nbar'></a>";
                result &= check('attribute-newline', e.firstChild.getAttribute('href') === '\nbar');
                e.innerHTML = '<!DOCTYPE html>';
                result &= check('doctype', e.firstChild === null);
                e.innerHTML = '\r';
                result &= check('cr-normalization', e.firstChild.nodeValue === '\n');
                e.innerHTML = '&lang;&rang;&apos;&ImaginaryI;&Kopf;&notinva;';
                result &= check('entities', e.firstChild.nodeValue === '\u27E8\u27E9\'\u2148\uD835\uDD42\u2209');
                e.innerHTML = '<?import namespace="foo" implementation="#bar">';
                result &= check('processing-instruction', e.firstChild.nodeType === 8 && e.firstChild.nodeValue === '?import namespace="foo" implementation="#bar"');
                e.innerHTML = '<!--foo--bar-->';
                result &= check('comment', e.firstChild.nodeType === 8 && e.firstChild.nodeValue === 'foo--bar');
                e.innerHTML = '<![CDATA[x]]>';
                result &= check('cdata', e.firstChild.nodeType === 8 && e.firstChild.nodeValue === '[CDATA[x]]');
                e.innerHTML = '<textarea><!--</textarea>--></textarea>';
                result &= check('textarea', e.firstChild.firstChild.nodeValue === '<!--');
                e.innerHTML = '<style><!--</style>--></style>';
                result &= check('style', e.firstChild.firstChild.nodeValue === '<!--');
                document.getElementById('status').textContent = result ? 'yes' : failures.join(',');
            </script></body>"##,
        );
        assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
        assert_eq!(
            dom.elements_named("div").next().unwrap().text_content(),
            "yes"
        );
    }

    #[test]
    fn drains_short_timers_before_layout() {
        let (dom, outcome) = execute_html(
            r#"<body><div id="status">waiting</div><script>
                setTimeout(() => document.getElementById('status').textContent = 'ready', 20);
            </script></body>"#,
        );
        assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
        assert_eq!(
            dom.elements_named("div").next().unwrap().text_content(),
            "ready"
        );
    }

    #[test]
    fn settles_bounded_one_second_startup_timers() {
        let (dom, outcome) = execute_html(
            r#"<body><div id="status">waiting</div><script>
                setTimeout(() => document.getElementById('status').textContent = 'ready', 500);
            </script></body>"#,
        );
        assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
        assert_eq!(
            dom.elements_named("div").next().unwrap().text_content(),
            "ready"
        );
    }

    #[test]
    fn settles_nested_startup_poll_within_the_explicit_horizon() {
        let (dom, outcome) = execute_html(
            r#"<body><div id="status">waiting</div><script>
                setTimeout(() => {
                    setTimeout(() => {
                        document.getElementById('status').textContent = 'ready';
                    }, 100);
                }, 1200);
            </script></body>"#,
        );
        assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
        assert_eq!(
            dom.elements_named("div").next().unwrap().text_content(),
            "ready"
        );
    }

    #[test]
    fn rescheduled_short_timer_does_not_starve_later_startup_timer() {
        let (dom, outcome) = execute_html(
            r#"<body><div id="status">waiting</div><script>
                function poll() { setTimeout(poll, 300); }
                setTimeout(poll, 300);
                setTimeout(() => document.getElementById('status').textContent = 'ready', 1000);
            </script></body>"#,
        );
        assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
        assert_eq!(
            dom.elements_named("div").next().unwrap().text_content(),
            "ready"
        );
    }

    #[test]
    fn runs_a_microtask_checkpoint_between_same_deadline_timer_tasks() {
        let (dom, outcome) = execute_html(
            r#"<body><div id="status">waiting</div><script>
                const order = [];
                setTimeout(() => {
                    order.push('timer-one');
                    queueMicrotask(() => order.push('microtask'));
                }, 0);
                setTimeout(() => {
                    order.push('timer-two');
                    document.getElementById('status').textContent = order.join(',');
                }, 0);
            </script></body>"#,
        );
        assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
        assert_eq!(
            dom.elements_named("div").next().unwrap().text_content(),
            "timer-one,microtask,timer-two"
        );
    }

    #[test]
    fn clear_timeout_cancels_the_rust_scheduled_task() {
        let (dom, outcome) = execute_html(
            r#"<body><div id="status">waiting</div><script>
                const cancelled = setTimeout(() => {
                    document.getElementById('status').textContent = 'cancelled task ran';
                }, 10);
                clearTimeout(cancelled);
                setTimeout(() => {
                    document.getElementById('status').textContent = 'ready';
                }, 10);
            </script></body>"#,
        );
        assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
        assert_eq!(
            dom.elements_named("div").next().unwrap().text_content(),
            "ready"
        );
    }

    #[test]
    fn clear_interval_stops_a_rescheduled_repeating_task() {
        let (dom, outcome) = execute_html(
            r#"<body><div id="status">waiting</div><script>
                let count = 0;
                const interval = setInterval(() => {
                    count++;
                    if (count === 3) {
                        clearInterval(interval);
                        document.getElementById('status').textContent = String(count);
                    }
                }, 10);
            </script></body>"#,
        );
        assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
        assert_eq!(
            dom.elements_named("div").next().unwrap().text_content(),
            "3"
        );
        assert!(
            outcome
                .diagnostics
                .iter()
                .all(|message| !message.contains("timers after settling")),
            "{:?}",
            outcome.diagnostics
        );
    }

    #[test]
    fn a_throwing_timer_does_not_prevent_the_next_task() {
        let (dom, outcome) = execute_html(
            r#"<body><div id="status">waiting</div><script>
                setTimeout(() => { throw new Error('expected timer failure'); }, 0);
                setTimeout(() => {
                    document.getElementById('status').textContent = 'ready';
                }, 0);
            </script></body>"#,
        );
        assert_eq!(
            dom.elements_named("div").next().unwrap().text_content(),
            "ready"
        );
        assert!(
            outcome
                .errors
                .iter()
                .any(|error| error.contains("expected timer failure")),
            "{:?}",
            outcome.errors
        );
    }

    #[test]
    fn dom_mutations_request_one_render_checkpoint() {
        let (_, mutating) = execute_html(
            r#"<body><div id="status"></div><script>
                setTimeout(() => {
                    const status = document.getElementById('status');
                    status.textContent = 'ready';
                    status.setAttribute('data-ready', 'true');
                }, 0);
            </script></body>"#,
        );
        assert!(mutating.errors.is_empty(), "{:?}", mutating.errors);
        assert_eq!(mutating.mutation_count, 2);
        assert!(mutating.render_requested);

        let (_, non_mutating) =
            execute_html(r#"<script>setTimeout(() => console.log('ready'), 0);</script>"#);
        assert!(non_mutating.errors.is_empty(), "{:?}", non_mutating.errors);
        assert_eq!(non_mutating.mutation_count, 0);
        assert!(!non_mutating.render_requested);
    }

    #[test]
    fn records_script_requested_navigation() {
        let (_, outcome) = execute_html(r#"<script>location.replace('/next?q=1')</script>"#);
        assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
        assert_eq!(
            outcome.navigation_url.as_deref(),
            Some("https://example.com/next?q=1")
        );
    }

    #[test]
    fn exposes_javascript_cookie_updates_to_the_network_layer() {
        let (dom, outcome) = execute_html(
            r#"<body><div id="value"></div><script>
                document.cookie = 'SG_SS=proof-token; Path=/; Secure; SameSite=None';
                document.cookie = 'theme=dark; Path=/';
                document.getElementById('value').textContent =
                    navigator.cookieEnabled + ':' + document.cookie;
            </script></body>"#,
        );
        assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
        assert_eq!(
            outcome.cookie_updates,
            [
                "SG_SS=proof-token; Path=/; Secure; SameSite=None",
                "theme=dark; Path=/"
            ]
        );
        assert_eq!(
            dom.elements_named("div").next().unwrap().text_content(),
            "true:SG_SS=proof-token; theme=dark"
        );
    }

    #[test]
    fn alternates_timers_and_promise_jobs_until_the_page_settles() {
        let (dom, outcome) = execute_html(
            r#"<body><div id="status">waiting</div><script>
                setTimeout(() => Promise.resolve().then(() => {
                    setTimeout(() => document.getElementById('status').textContent = 'ready', 0);
                }), 0);
            </script></body>"#,
        );
        assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
        assert_eq!(
            dom.elements_named("div").next().unwrap().text_content(),
            "ready"
        );
    }

    #[test]
    fn exposes_browser_base64_helpers() {
        let (dom, outcome) = execute_html(
            r#"<body><div id="value"></div><script>
                document.getElementById('value').textContent =
                    atob('SGVsbG8h') + ':' + btoa('Rust');
            </script></body>"#,
        );
        assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
        assert_eq!(
            dom.elements_named("div").next().unwrap().text_content(),
            "Hello!:UnVzdA=="
        );
    }

    #[test]
    fn exposes_legacy_substr_for_web_compatibility() {
        let (dom, outcome) = execute_html(
            r#"<body><div id="value"></div><script>
                document.getElementById('value').textContent = [
                    'https://example.com'.substr(0, 5),
                    'abcdef'.substr(-3, 2),
                    'abcdef'.substr(2)
                ].join('|');
            </script></body>"#,
        );
        assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
        assert_eq!(
            dom.elements_named("div").next().unwrap().text_content(),
            "https|de|cdef"
        );
    }

    #[test]
    fn contains_evaluator_panics_in_promise_jobs() {
        let (_, outcome) = execute_html(
            r#"<script>
                Promise.resolve().then(() => { for (;;) {} });
            </script>"#,
        );
        assert!(
            outcome
                .errors
                .iter()
                .any(|error| error.contains("stopped safely")
                    || error.contains("maximum number of iteration loops")),
            "{:?}",
            outcome.errors
        );
        assert!(outcome.runtime_stopped);
    }

    #[test]
    fn exposes_a_same_origin_iframe_browsing_context() {
        let (dom, outcome) = execute_html(
            r#"<body><div id="value">no</div><script>
                const frame = document.createElement('iframe');
                document.body.appendChild(frame);
                if (frame.contentWindow !== window &&
                    frame.contentWindow.window === frame.contentWindow &&
                    frame.contentWindow.parent === window &&
                    frame.contentWindow.Array !== Array &&
                    frame.contentDocument !== document &&
                    frame.contentDocument.defaultView === frame.contentWindow &&
                    document.defaultView === window) {
                    document.getElementById('value').textContent = 'yes';
                }
                frame.remove();
            </script></body>"#,
        );
        assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
        assert_eq!(
            dom.elements_named("div").next().unwrap().text_content(),
            "yes"
        );
    }
}
