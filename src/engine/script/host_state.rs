//! Per-document native state shared with the V8 realm.

use super::*;
use crate::engine::MediaEnvironment;

mod cookies;
pub(crate) mod geometry;
mod ownership;
mod scripts;
mod storage;

use crate::storage::{StorageAreaState, StorageMutation};

#[derive(Debug)]
pub(super) struct PendingModuleEvaluation {
    pub(super) source_url: String,
}

#[derive(Debug)]
pub(super) struct CompletedModuleEvaluation {
    pub(super) pending: PendingModuleEvaluation,
    pub(super) result: Result<(), String>,
}

pub(super) struct HostState {
    pub(super) document: NodeRef,
    pub(super) document_url: String,
    pub(super) pointer_path: Vec<NodeRef>,
    pub(super) document_character_set: String,
    pub(super) stylesheet_sources: Vec<(String, String)>,
    pub(super) module_loader: Rc<module_loader::WebModuleLoader>,
    pub(super) nodes: HashMap<u32, NodeRef>,
    pub(super) node_ids: HashMap<NodeId, u32>,
    pub(super) owner_documents: HashMap<NodeId, u64>,
    pub(super) document_roots: HashMap<u64, NodeRef>,
    pub(super) html_documents: HashSet<u64>,
    pub(super) template_contents_documents: HashMap<u64, u64>,
    pub(super) next_node_id: u32,
    pub(super) mutation_count: usize,
    pub(super) task_mutations: task_mutation_profile::TaskMutationProfile,
    pub(super) console: Vec<String>,
    pub(super) navigation_url: Option<String>,
    pub(super) viewport_scroll_y: Option<f32>,
    pub(super) history_actions: Vec<ScriptHistoryAction>,
    pub(super) cookie_header: String,
    pub(super) cookie_version: u64,
    pub(super) cookie_updates: Vec<String>,
    pub(super) local_storage: StorageAreaState,
    pub(super) session_storage: StorageAreaState,
    pub(super) storage_updates: Vec<StorageMutation>,
    pub(super) executed: usize,
    pub(super) diagnostics: Vec<String>,
    pub(super) host_call_profile: super::host_profiling::HostCallProfile,
    pub(super) pending_document_write: String,
    pub(super) pending_dynamic_scripts: super::dynamic_scripts::queue::ScriptQueue,
    pub(super) next_module_evaluation_id: u32,
    pub(super) pending_module_evaluations: HashMap<u32, PendingModuleEvaluation>,
    pub(super) completed_module_evaluations: Vec<CompletedModuleEvaluation>,
    pub(super) document_load: super::runtime::document_lifecycle::DocumentLoad,
    pub(super) next_fetch_id: u32,
    pub(super) pending_fetch_actions: Vec<ScriptFetchAction>,
    pub(super) next_worker_id: u32,
    pub(super) pending_worker_actions: Vec<ScriptWorkerAction>,
    pub(super) pending_fullscreen_actions: Vec<ScriptFullscreenAction>,
    pub(super) pending_media_actions: Vec<ScriptMediaAction>,
    pub(super) timers: EventLoopScheduler<u32>,
    pub(super) timer_handles: HashMap<u32, TaskHandle>,
    pub(super) computed_styles: Option<(u64, StyleSet)>,
    pub(super) offset_parent_styles: Option<(u64, StyleSet)>,
    /// Latest renderer layout border boxes, exposed through CSSOM View geometry APIs.
    pub(super) layout_geometry: HashMap<NodeId, RectF>,
    pub(super) layout_geometry_version: u64,
    pub(super) layout_geometry_initialized: bool,
    pub(super) layout_flush: Option<LayoutFlushCallback>,
    pub(super) media_environment: MediaEnvironment,
    pub(super) layout_viewport_width: f32,
    pub(super) layout_viewport_height: f32,
    pub(super) layout_content_height: f32,
    pub(super) quirks_mode: bool,
    pub(super) pending_invalidation: render_invalidation::PendingInvalidation,
    pub(super) pending_layout_invalidation: render_invalidation::PendingInvalidation,
}

impl HostState {
    pub(super) fn new(
        document: NodeRef,
        document_url: &str,
        character_set: &str,
        module_loader: Rc<module_loader::WebModuleLoader>,
    ) -> Self {
        let mut state = Self {
            document,
            document_url: document_url.to_string(),
            document_character_set: character_set.to_string(),
            stylesheet_sources: Vec::new(),
            pointer_path: Vec::new(),
            module_loader,
            nodes: HashMap::new(),
            node_ids: HashMap::new(),
            owner_documents: HashMap::new(),
            document_roots: HashMap::new(),
            html_documents: HashSet::new(),
            template_contents_documents: HashMap::new(),
            next_node_id: 1,
            mutation_count: 0,
            task_mutations: task_mutation_profile::TaskMutationProfile::default(),
            console: Vec::new(),
            navigation_url: None,
            viewport_scroll_y: None,
            history_actions: Vec::new(),
            cookie_header: String::new(),
            cookie_version: 1,
            cookie_updates: Vec::new(),
            local_storage: StorageAreaState::default(),
            session_storage: StorageAreaState::default(),
            storage_updates: Vec::new(),
            executed: 0,
            diagnostics: Vec::new(),
            host_call_profile: super::host_profiling::HostCallProfile::default(),
            pending_document_write: String::new(),
            pending_dynamic_scripts: Default::default(),
            next_module_evaluation_id: 1,
            pending_module_evaluations: HashMap::new(),
            completed_module_evaluations: Vec::new(),
            document_load: Default::default(),
            next_fetch_id: 1,
            pending_fetch_actions: Vec::new(),
            next_worker_id: 1,
            pending_worker_actions: Vec::new(),
            pending_fullscreen_actions: Vec::new(),
            pending_media_actions: Vec::new(),
            timers: EventLoopScheduler::new(),
            timer_handles: HashMap::new(),
            computed_styles: None,
            offset_parent_styles: None,
            layout_geometry: HashMap::new(),
            layout_geometry_version: 0,
            layout_geometry_initialized: false,
            layout_flush: None,
            media_environment: MediaEnvironment::new(1280.0, 720.0, 1.0, false),
            layout_viewport_width: 1280.0,
            layout_viewport_height: 720.0,
            layout_content_height: 720.0,
            quirks_mode: false,
            pending_invalidation: render_invalidation::PendingInvalidation::default(),
            pending_layout_invalidation: render_invalidation::PendingInvalidation::default(),
        };
        let document = state.document.clone();
        state.register_document(document, true);
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

    pub(super) fn resolved_url(&self, reference: &str) -> String {
        resolve_url(&self.document_url, reference).unwrap_or_else(|| reference.to_string())
    }

    pub(super) fn diagnose(&mut self, message: String) {
        if self.diagnostics.len() < 64 {
            self.diagnostics.push(message);
        }
    }

    pub(super) fn append_host_call_diagnostics(&mut self, diagnostics: &mut Vec<String>) {
        diagnostics.extend(self.host_call_profile.take_diagnostics());
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
        self.task_mutations.record(kind);
        self.invalidate_style_rules_for_mutation(target, kind);
        if requires_render {
            self.pending_invalidation
                .record(&self.document, target, kind);
            self.pending_layout_invalidation
                .record(&self.document, target, kind);
            self.timers.request_render();
        }
    }

    pub(super) fn begin_task(&mut self) {
        self.task_mutations.reset();
    }

    pub(super) fn extend_invalidation_root(&mut self, target: &NodeRef) {
        self.pending_invalidation.extend(&self.document, target);
        self.pending_layout_invalidation
            .extend(&self.document, target);
    }

    pub(super) fn record_removed_subtree(&mut self, root: &NodeRef) {
        self.pending_invalidation.record_removed_subtree(root);
        self.pending_layout_invalidation
            .record_removed_subtree(root);
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
            current = node.shadow_including_parent();
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

    pub(super) fn schedule_idle_callback(&mut self, id: u32, delay: Duration) {
        if let Some(previous) = self.timer_handles.remove(&id) {
            self.timers.cancel(previous);
        }
        let handle = self.timers.queue_task(TaskSource::IdleTask, delay, id);
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
            current = node.shadow_including_parent();
        }
        false
    }
}
