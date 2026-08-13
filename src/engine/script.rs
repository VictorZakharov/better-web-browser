use super::dom::{Node, NodeData, NodeRef};
use crate::navigation::resolve_url;
use boa_engine::{
    Context, JsNativeError, JsResult, JsString, JsValue, NativeFunction, Source,
    property::Attribute,
};
use std::cell::RefCell;
use std::collections::HashMap;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::rc::Rc;
use std::time::Instant;

const MAX_LOOP_ITERATIONS: u64 = if cfg!(test) { 25_000 } else { 5_000_000 };
const MAX_SCRIPT_BYTES: usize = 8 * 1024 * 1024;

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
}

struct HostState {
    document: NodeRef,
    document_url: String,
    nodes: HashMap<u32, NodeRef>,
    node_ids: HashMap<usize, u32>,
    next_node_id: u32,
    mutation_count: usize,
    console: Vec<String>,
    navigation_url: Option<String>,
    cookies: HashMap<String, String>,
    cookie_updates: Vec<String>,
    executed: usize,
    diagnostics: Vec<String>,
}

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
        };
        for node in Node::descendants(&state.document).collect::<Vec<_>>() {
            state.id_for(&node);
        }
        state
    }

    fn id_for(&mut self, node: &NodeRef) -> u32 {
        let pointer = Rc::as_ptr(node) as usize;
        if let Some(id) = self.node_ids.get(&pointer) {
            return *id;
        }
        let id = self.next_node_id;
        self.next_node_id = self.next_node_id.saturating_add(1);
        self.node_ids.insert(pointer, id);
        self.nodes.insert(id, node.clone());
        id
    }

    fn node(&self, id: u32) -> Option<NodeRef> {
        self.nodes.get(&id).cloned()
    }

    fn register_subtree(&mut self, root: &NodeRef) {
        for node in Node::descendants(root) {
            self.id_for(&node);
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

thread_local! {
    static ACTIVE_HOST: RefCell<Option<HostState>> = const { RefCell::new(None) };
}

pub fn execute(document: NodeRef, document_url: &str, scripts: &[ScriptInput]) -> ScriptOutcome {
    if scripts.is_empty() {
        return ScriptOutcome::default();
    }

    ACTIVE_HOST.with(|host| {
        *host.borrow_mut() = Some(HostState::new(document, document_url));
    });

    // Keep ownership outside the unwind boundary. Some evaluator failures leave Boa's
    // garbage-collected maps borrowed; dropping that damaged context while the first panic is
    // unwinding can trigger a second panic and abort the whole browser process.
    let mut context = Box::new(Context::default());
    let result = catch_unwind(AssertUnwindSafe(|| execute_inner(scripts, &mut context)));
    if result.is_err() {
        // The context is not safe to finalize after an internal evaluator panic. This path stops
        // the runtime and falls back to the pre-script DOM, so retaining this one failed context
        // is preferable to allowing a double-panic process abort.
        std::mem::forget(context);
    }
    let outcome = match result {
        Ok(outcome) => outcome,
        Err(payload) => {
            let detail = payload
                .downcast_ref::<&str>()
                .map(|message| (*message).to_string())
                .or_else(|| payload.downcast_ref::<String>().cloned())
                .unwrap_or_else(|| "unknown evaluator panic".to_string());
            ScriptOutcome {
                errors: vec![format!(
                    "JavaScript runtime was stopped safely after an evaluator failure: {detail}"
                )],
                runtime_stopped: true,
                ..ScriptOutcome::default()
            }
        }
    };
    finish_host(outcome)
}

fn execute_inner(scripts: &[ScriptInput], context: &mut Context) -> ScriptOutcome {
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

    let mut total_bytes = 0_usize;
    for script in scripts {
        if total_bytes.saturating_add(script.code.len()) > MAX_SCRIPT_BYTES {
            outcome.errors.push(format!(
                "{}: skipped because the page exceeds the {} MiB JavaScript limit",
                script.source_url,
                MAX_SCRIPT_BYTES / 1024 / 1024
            ));
            break;
        }
        total_bytes += script.code.len();

        let node_id = ACTIVE_HOST.with(|host| {
            host.borrow_mut()
                .as_mut()
                .map(|state| state.id_for(&script.node))
                .unwrap_or_default()
        });
        let current_script = format!("document.__setCurrentScript({node_id});");
        if let Err(error) = context.eval(Source::from_bytes(&current_script)) {
            outcome.errors.push(format!(
                "{}: set document.currentScript: {error}",
                script.source_url
            ));
        }

        let script_started = Instant::now();
        match context.eval(Source::from_bytes(&script.code)) {
            Ok(_) => {
                outcome.executed += 1;
                ACTIVE_HOST.with(|host| {
                    if let Some(state) = host.borrow_mut().as_mut() {
                        state.executed += 1;
                    }
                });
                if let Err(error) = context.run_jobs() {
                    outcome
                        .errors
                        .push(format!("{}: promise job: {error}", script.source_url));
                }
            }
            Err(error) => outcome
                .errors
                .push(format!("{}: {error}", script.source_url)),
        }
        let script_time = script_started.elapsed();
        if script_time.as_millis() >= 1 {
            outcome.diagnostics.push(format!(
                "JavaScript {:.3} ms: {}",
                script_time.as_secs_f64() * 1_000.0,
                script.source_url
            ));
        }
    }

    let finish_lifecycle = scripts.iter().any(|script| script.finish_lifecycle);
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
    for _ in 0..4 {
        if let Err(error) = context.eval(Source::from_bytes("__drainTimers();")) {
            outcome
                .errors
                .push(format!("settle JavaScript timers: {error}"));
            break;
        }
        if let Err(error) = context.run_jobs() {
            outcome
                .errors
                .push(format!("settle JavaScript promise jobs: {error}"));
            break;
        }
    }

    if let Ok(value) = context.eval(Source::from_bytes("__pendingTimerSummary();"))
        && let Ok(summary) = value.to_string(context)
    {
        let summary = summary.to_std_string_escaped();
        if !summary.is_empty() {
            outcome
                .diagnostics
                .push(format!("JavaScript timers after settling: {summary}"));
        }
    }

    outcome
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

fn finish_host(mut outcome: ScriptOutcome) -> ScriptOutcome {
    if let Some(state) = ACTIVE_HOST.with(|host| host.borrow_mut().take()) {
        outcome.mutation_count = state.mutation_count;
        outcome.executed = outcome.executed.max(state.executed);
        outcome.console.extend(state.console);
        outcome.diagnostics.extend(state.diagnostics);
        outcome.navigation_url = state.navigation_url;
        outcome.cookie_updates = state.cookie_updates;
    }
    outcome
}

fn host_call(_this: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let operation = argument_string(args, 0, context)?;
    ACTIVE_HOST.with(|host| {
        let mut host = host.borrow_mut();
        let state = host
            .as_mut()
            .ok_or_else(|| JsNativeError::typ().with_message("browser host is not active"))?;

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
                        NodeData::Document => 9,
                        NodeData::Doctype { .. } => 10,
                        NodeData::ProcessingInstruction { .. } => 7,
                    })
                    .unwrap_or_default();
                Ok(JsValue::from(kind))
            }
            "tagName" => {
                let value = state
                    .node(argument_id(args, 1))
                    .and_then(|node| node.tag_name().map(str::to_ascii_uppercase))
                    .unwrap_or_default();
                Ok(js_string(value))
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
                let node = Node::create_element(&tag_name);
                Ok(JsValue::from(state.id_for(&node)))
            }
            "createText" => {
                let contents = argument_string(args, 1, context)?;
                let node = Node::create_text(&contents);
                Ok(JsValue::from(state.id_for(&node)))
            }
            "createComment" => {
                let contents = argument_string(args, 1, context)?;
                let node = Node::create_comment(&contents);
                Ok(JsValue::from(state.id_for(&node)))
            }
            "appendChild" => {
                let parent = state.node(argument_id(args, 1));
                let child = state.node(argument_id(args, 2));
                let changed = parent
                    .zip(child.clone())
                    .is_some_and(|(parent, child)| Node::append_child(&parent, child));
                if changed {
                    state.mutation_count += 1;
                    if let (Some(parent), Some(child)) = (
                        state.node(argument_id(args, 1)),
                        state.node(argument_id(args, 2)),
                    ) {
                        state.diagnose(format!(
                            "append {} to {}",
                            node_label(&child),
                            node_label(&parent)
                        ));
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
                        |((parent, child), reference)| {
                            Node::insert_before(&parent, child, &reference)
                        },
                    )
                };
                if changed {
                    state.mutation_count += 1;
                    state.diagnose("insert node before sibling".into());
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
                    state.mutation_count += 1;
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
                    state.mutation_count += 1;
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
                    state.mutation_count += 1;
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
                    state.mutation_count += 1;
                    if let Some(node) = state.node(argument_id(args, 1)) {
                        state.diagnose(format!("set {} on {}", name, node_label(&node)));
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
                    state.mutation_count += 1;
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
                    state.mutation_count += 1;
                    if let Some(node) = state.node(argument_id(args, 1)) {
                        state.diagnose(format!("replace innerHTML of {}", node_label(&node)));
                    }
                }
                Ok(JsValue::from(changed))
            }
            "innerHtmlAppend" => {
                let html = argument_string(args, 2, context)?;
                let changed = if let Some(node) = state.node(argument_id(args, 1)) {
                    let holder = Node::create_element("div");
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
                    state.mutation_count += 1;
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
    })
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
    let Some(index) = children.iter().position(|child| Rc::ptr_eq(child, &node)) else {
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
    let mut result = Vec::new();
    for node in Node::descendants(root).skip(1) {
        if node.element().is_none() {
            continue;
        }
        if groups.iter().any(|group| matches_selector(&node, group))
            && !result.iter().any(|existing| Rc::ptr_eq(existing, &node))
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
                            .is_none_or(|child| !Rc::ptr_eq(child, node))
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
                            .is_none_or(|child| !Rc::ptr_eq(child, node))
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
    for child in node.children.borrow().iter() {
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
            event.target ||= this;
            event.currentTarget = this;
            const bucket = listenerStore.get(this)?.get(event.type) || [];
            for (const callback of [...bucket]) {
                if (typeof callback === 'function') callback.call(this, event);
                else callback.handleEvent(event);
                if (event.__immediate) break;
            }
            const handler = this['on' + event.type];
            if (!event.__immediate && typeof handler === 'function') handler.call(this, event);
            return !event.defaultPrevented;
        }
    }

    class Node extends EventTarget {
        constructor(id) {
            super();
            this.__id = id;
        }
        get nodeType() { return host('nodeType', this.__id); }
        get nodeName() { return this.nodeType === 1 ? host('tagName', this.__id) : this.nodeType === 9 ? '#document' : this.nodeType === 3 ? '#text' : '#comment'; }
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
        get localName() { return this.tagName.toLowerCase(); }
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
            const names = [];
            for (const match of this.outerHTML.matchAll(/\s+([^\s=/>]+)/g)) names.push(match[1]);
            return names;
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
        click() { this.dispatchEvent(new Event('click', { bubbles: true, cancelable: true })); }
        focus() { document.activeElement = this; this.dispatchEvent(new Event('focus')); }
        blur() { if (document.activeElement === this) document.activeElement = document.body; this.dispatchEvent(new Event('blur')); }
        get clientWidth() { return 0; }
        get clientHeight() { return 0; }
        get offsetWidth() { return 0; }
        get offsetHeight() { return 0; }
        getBoundingClientRect() { return { x: 0, y: 0, top: 0, left: 0, right: 0, bottom: 0, width: 0, height: 0, toJSON() { return this; } }; }
    }

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
        const node = type === 9 ? new Document(id) : type === 1 ? new Element(id) : type === 8 ? new Comment(id) : new Text(id);
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
    windowObject.HTMLElement = Element;
    windowObject.Document = Document;
    windowObject.Text = Text;
    windowObject.Event = Event;
    windowObject.CustomEvent = CustomEvent;
    windowObject.EventTarget = EventTarget;
    windowObject.DOMTokenList = DOMTokenList;
    windowObject.CSSStyleDeclaration = CSSStyleDeclaration;
    windowObject.addEventListener = windowEvents.addEventListener.bind(windowEvents);
    windowObject.removeEventListener = windowEvents.removeEventListener.bind(windowEvents);
    windowObject.dispatchEvent = windowEvents.dispatchEvent.bind(windowEvents);

    const iframeWindow = isolatedIframeWindow || windowObject;
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
        timers.set(id, { callback, delay: Math.max(0, Number(delay) || 0), repeat, args });
        return id;
    };
    windowObject.setTimeout = (callback, delay, ...args) => queueTimer(callback, delay, false, args);
    windowObject.setInterval = (callback, delay, ...args) => queueTimer(callback, delay, true, args);
    windowObject.clearTimeout = windowObject.clearInterval = id => timers.delete(Number(id));
    windowObject.requestAnimationFrame = callback => queueTimer(() => callback(performance.now()), 16, false, []);
    windowObject.cancelAnimationFrame = windowObject.clearTimeout;
    windowObject.queueMicrotask = callback => Promise.resolve().then(callback);
    windowObject.__drainTimers = (maxDelay = 100, maxCallbacks = 128) => {
        let count = 0;
        while (count < maxCallbacks) {
            const next = [...timers]
                .filter(([, timer]) => !timer.repeat && timer.delay <= maxDelay)
                .sort((a, b) => a[1].delay - b[1].delay)[0];
            if (!next) break;
            const [id, timer] = next;
            timers.delete(id);
            if (typeof timer.callback === 'function') timer.callback(...timer.args);
            else (0, eval)(String(timer.callback));
            count++;
        }
    };
    windowObject.__pendingTimerSummary = () => [...timers]
        .map(([id, timer]) => id + '@' + timer.delay + (timer.repeat ? 'r' : ''))
        .join(',');

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

    windowObject.getComputedStyle = element => element?.style || styleProxy(document.createElement('div'));
    windowObject.matchMedia = query => ({ media: String(query), matches: false, onchange: null, addListener() {}, removeListener() {}, addEventListener() {}, removeEventListener() {}, dispatchEvent() { return true; } });
    windowObject.CSS = { supports() { return false; }, escape(value) { return String(value).replace(/[^a-zA-Z0-9_-]/g, match => '\\' + match); } };
    windowObject.Image = class Image extends Element {
        constructor() { const element = document.createElement('img'); return element; }
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
        windowObject.__drainTimers();
        document.readyState = 'complete';
        windowObject.dispatchEvent(new Event('load'));
        windowObject.__drainTimers();
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
