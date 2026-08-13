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

// These responsibility-based chunks are concatenated before Boa parses the bootstrap IIFE.
const BROWSER_BOOTSTRAP: &str = concat!(
    include_str!("script/bootstrap/core.js"),
    include_str!("script/bootstrap/elements.js"),
    include_str!("script/bootstrap/forms.js"),
    include_str!("script/bootstrap/document.js"),
    include_str!("script/bootstrap/platform.js"),
    include_str!("script/bootstrap/tasks.js"),
);

#[cfg(test)]
#[path = "script/tests/mod.rs"]
mod tests;
