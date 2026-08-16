//! Per-document native state shared with the Boa realm.

use super::*;

#[derive(Debug, Clone)]
pub(super) struct PendingDynamicScript {
    pub(super) node: NodeRef,
    pub(super) source_url: String,
}

pub(super) struct HostState {
    pub(super) document: NodeRef,
    pub(super) document_url: String,
    pub(super) document_character_set: String,
    pub(super) nodes: HashMap<u32, NodeRef>,
    pub(super) node_ids: HashMap<NodeId, u32>,
    pub(super) owner_documents: HashMap<NodeId, u64>,
    pub(super) document_roots: HashMap<u64, NodeRef>,
    pub(super) html_documents: HashSet<u64>,
    pub(super) next_node_id: u32,
    pub(super) mutation_count: usize,
    pub(super) task_mutation_count: usize,
    pub(super) console: Vec<String>,
    pub(super) navigation_url: Option<String>,
    pub(super) cookies: HashMap<String, String>,
    pub(super) cookie_updates: Vec<String>,
    pub(super) executed: usize,
    pub(super) diagnostics: Vec<String>,
    pub(super) pending_document_write: String,
    pub(super) pending_dynamic_scripts: Vec<PendingDynamicScript>,
    pub(super) started_dynamic_scripts: HashSet<NodeId>,
    pub(super) timers: EventLoopScheduler<u32>,
    pub(super) timer_handles: HashMap<u32, TaskHandle>,
    pub(super) computed_styles: Option<(u64, StyleSet)>,
    pub(super) pending_invalidation: render_invalidation::PendingInvalidation,
}

/// A Boa context owns only a weak link to native document state. If an evaluator panic requires
/// leaking the damaged context, the page DOM and scheduler can still be released normally.
#[derive(Clone, Finalize, JsData, Trace)]
#[boa_gc(unsafe_empty_trace)]
pub(super) struct HostStateLink(pub(super) Weak<RefCell<HostState>>);

impl HostState {
    pub(super) fn new(document: NodeRef, document_url: &str, character_set: &str) -> Self {
        let document_identity = document.id().document();
        let mut state = Self {
            document,
            document_url: document_url.to_string(),
            document_character_set: character_set.to_string(),
            nodes: HashMap::new(),
            node_ids: HashMap::new(),
            owner_documents: HashMap::new(),
            document_roots: HashMap::new(),
            html_documents: HashSet::new(),
            next_node_id: 1,
            mutation_count: 0,
            task_mutation_count: 0,
            console: Vec::new(),
            navigation_url: None,
            cookies: HashMap::new(),
            cookie_updates: Vec::new(),
            executed: 0,
            diagnostics: Vec::new(),
            pending_document_write: String::new(),
            pending_dynamic_scripts: Vec::new(),
            started_dynamic_scripts: HashSet::new(),
            timers: EventLoopScheduler::new(),
            timer_handles: HashMap::new(),
            computed_styles: None,
            pending_invalidation: render_invalidation::PendingInvalidation::default(),
        };
        let document = state.document.clone();
        state
            .document_roots
            .insert(document_identity, document.clone());
        state.html_documents.insert(document_identity);
        state.register_subtree(&document);
        state
    }

    pub(super) fn id_for(&mut self, node: &NodeRef) -> u32 {
        let node_id = node.id();
        if let Some(id) = self.node_ids.get(&node_id) {
            return *id;
        }
        if self.nodes.len() >= MAX_DOM_NODES {
            self.diagnose(format!(
                "DOM node registry reached the {MAX_DOM_NODES}-node limit"
            ));
            return 0;
        }
        let id = self.next_node_id;
        self.next_node_id = self.next_node_id.saturating_add(1);
        self.node_ids.insert(node_id, id);
        self.nodes.insert(id, node.clone());
        id
    }

    pub(super) fn node(&self, id: u32) -> Option<NodeRef> {
        self.nodes.get(&id).cloned()
    }

    pub(super) fn ensure_node_capacity(&self, additional: usize) -> JsResult<()> {
        if self.nodes.len().saturating_add(additional) <= MAX_DOM_NODES {
            Ok(())
        } else {
            Err(JsNativeError::range()
                .with_message(format!(
                    "DOM node budget of {MAX_DOM_NODES} would be exceeded"
                ))
                .into())
        }
    }

    pub(super) fn document_for(&self, node: &NodeRef) -> Option<NodeRef> {
        self.document_roots
            .get(&self.owner_document_identity(node))
            .cloned()
    }

    pub(super) fn is_html_document_for(&self, node: &NodeRef) -> bool {
        self.html_documents
            .contains(&self.owner_document_identity(node))
    }

    pub(super) fn register_document(&mut self, document: NodeRef, html: bool) -> u32 {
        let identity = document.id().document();
        self.document_roots.insert(identity, document.clone());
        if html {
            self.html_documents.insert(identity);
        }
        self.register_subtree(&document);
        self.id_for(&document)
    }

    pub(super) fn adopt_subtree(&mut self, parent: &NodeRef, child: &NodeRef) {
        let owner_identity = self.owner_document_identity(parent);
        let mut stack = vec![child.clone()];
        while let Some(node) = stack.pop() {
            self.owner_documents.insert(node.id(), owner_identity);
            stack.extend(node.children.borrow().iter().rev().cloned());
            if let Some(contents) = node
                .element()
                .and_then(|element| element.template_contents.borrow().clone())
            {
                stack.push(contents);
            }
        }
    }

    pub(super) fn register_subtree(&mut self, root: &NodeRef) {
        let owner_identity = root
            .parent()
            .map(|parent| self.owner_document_identity(&parent))
            .unwrap_or_else(|| self.owner_document_identity(root));
        let mut stack = vec![root.clone()];
        while let Some(node) = stack.pop() {
            self.owner_documents.insert(node.id(), owner_identity);
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

    fn owner_document_identity(&self, node: &NodeRef) -> u64 {
        self.owner_documents
            .get(&node.id())
            .copied()
            .unwrap_or_else(|| node.id().document())
    }

    pub(super) fn resolved_url(&self, reference: &str) -> String {
        resolve_url(&self.document_url, reference).unwrap_or_else(|| reference.to_string())
    }

    pub(super) fn diagnose(&mut self, message: String) {
        if self.diagnostics.len() < 64 {
            self.diagnostics.push(message);
        }
    }

    pub(super) fn record_mutation(&mut self, target: Option<&NodeRef>, kind: MutationKind<'_>) {
        let requires_render = target.is_some_and(|target| self.mutation_requires_render(target));
        self.record_mutation_with_render(target, kind, requires_render);
    }

    pub(super) fn record_mutation_with_render(
        &mut self,
        target: Option<&NodeRef>,
        kind: MutationKind<'_>,
        requires_render: bool,
    ) {
        self.mutation_count += 1;
        self.task_mutation_count += 1;
        if requires_render {
            self.pending_invalidation
                .record(&self.document, target, kind);
            self.timers.request_render();
        }
    }

    pub(super) fn begin_task(&mut self) {
        self.task_mutation_count = 0;
    }

    pub(super) fn extend_invalidation_root(&mut self, target: &NodeRef) {
        self.pending_invalidation.extend(target);
    }

    pub(super) fn record_removed_subtree(&mut self, root: &NodeRef) {
        self.pending_invalidation.record_removed_subtree(root);
    }

    pub(super) fn mutation_requires_render(&self, target: &NodeRef) -> bool {
        let mut current = Some(target.clone());
        let mut connected = false;
        while let Some(node) = current {
            if node.tag_name() == Some("script") {
                return false;
            }
            if node.id() == self.document.id() {
                connected = true;
                break;
            }
            current = node.parent();
        }
        connected
    }

    pub(super) fn schedule_timer(&mut self, id: u32, delay: Duration, repeat: bool) {
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

    pub(super) fn cancel_timer(&mut self, id: u32) -> bool {
        self.timer_handles
            .remove(&id)
            .is_some_and(|handle| self.timers.cancel(handle))
    }

    pub(super) fn take_ready_timer(&mut self) -> Option<u32> {
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

    pub(super) fn timer_summary(&self) -> String {
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

    pub(super) fn is_connected(&self, node: &NodeRef) -> bool {
        let mut current = Some(node.clone());
        while let Some(node) = current {
            if node.id() == self.document.id() {
                return true;
            }
            current = node.parent();
        }
        false
    }

    pub(super) fn queue_dynamic_script(&mut self, node: &NodeRef) {
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

    pub(super) fn cookie_header(&self) -> String {
        let mut cookies = self.cookies.iter().collect::<Vec<_>>();
        cookies.sort_unstable_by(|left, right| left.0.cmp(right.0));
        cookies
            .into_iter()
            .map(|(name, value)| format!("{name}={value}"))
            .collect::<Vec<_>>()
            .join("; ")
    }

    pub(super) fn replace_cookies_from_header(&mut self, cookie_header: &str) {
        self.cookies.clear();
        for pair in cookie_header.split(';').map(str::trim) {
            let Some((name, value)) = pair.split_once('=') else {
                continue;
            };
            let name = name.trim();
            if !name.is_empty() {
                self.cookies
                    .insert(name.to_string(), value.trim().to_string());
            }
        }
    }

    pub(super) fn set_cookie(&mut self, assignment: String) {
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
