//! Retained JavaScript realm ownership and guarded incremental execution.

use super::dynamic_scripts::drain_one_dynamic_script;
use super::execution::{execute_additional_inner, execute_inner};
use super::runtime_guard::{
    finish_host, inactive_runtime_outcome, lifecycle_error, panic_detail, stopped_runtime_outcome,
};
use super::timer_execution::{TimerSlice, settle_timer_slice};
use super::*;
use std::panic::{AssertUnwindSafe, catch_unwind};

mod animations;
mod canvas_presentation;
mod capture;
mod completions;
pub(super) mod document_lifecycle;
mod document_streams;
mod dynamic_modules;
mod dynamic_scripts;
mod frames;
mod graph_audio;
pub(crate) use frames::FramePaintSnapshot;
mod element_images;
mod font_environment;
mod geometry;
mod guarded_run;
mod history;
mod import_maps;
mod media_images;
mod memory;
mod module_preparation;
pub(crate) mod parser;
pub(crate) mod parser_queue;
mod permission;
pub(crate) mod platform_info;
mod restart;
mod scheduling;
pub(crate) use restart::RestartState;
mod storage;

/// Owns one document's JavaScript realm and all native state that must remain on the realm's
/// creating thread. Embedders must keep this runtime and its document together on that owner
/// thread for the complete document lifetime.
pub struct ScriptRuntime {
    context: Option<Box<Context>>,
    pub(super) host: Rc<RefCell<HostState>>,
    total_script_bytes: Rc<std::cell::Cell<usize>>,
    initialized: bool,
    prefer_timer_task: bool,
    last_heap_sample: Option<Instant>,
    frames: Option<frames::ChildRuntimes>,
}

impl ScriptRuntime {
    /// Mirrors the owning document's sticky media activation before a trusted input is dispatched.
    pub(crate) fn set_audio_activation(&mut self, activated: bool) {
        self.host.borrow_mut().audio_activated = activated;
    }

    pub(crate) fn set_notification_permission(
        &mut self,
        permission: crate::renderer_protocol::NotificationPermission,
    ) {
        self.host.borrow_mut().notification_permission = permission;
    }
    pub(crate) fn process_parser_csp_meta(
        &mut self,
        node: &NodeRef,
    ) -> Result<ScriptOutcome, String> {
        let policy_updates = {
            let mut host = self.host.borrow_mut();
            host.process_inserted_csp_meta(node)?;
            std::mem::take(&mut host.pending_policy_updates)
        };
        if !policy_updates.is_empty() {
            self.context
                .as_deref_mut()
                .expect("parser CSP requires an active realm")
                .refresh_code_generation_policy();
        }
        Ok(ScriptOutcome {
            policy_updates,
            ..ScriptOutcome::default()
        })
    }

    pub(crate) fn set_document_policy(
        &mut self,
        policy: std::sync::Arc<crate::fetch::csp::PolicyContainer>,
    ) {
        self.host.borrow_mut().policy = policy;
        self.context
            .as_deref_mut()
            .expect("document CSP requires an active realm")
            .refresh_code_generation_policy();
    }

    pub(crate) fn document_url(&self) -> String {
        self.host.borrow().document_url.clone()
    }

    pub(crate) fn focused_node_id(&self) -> Option<crate::engine::dom::NodeId> {
        self.host
            .borrow()
            .focused_node
            .as_ref()
            .map(|node| node.id())
    }

    pub fn new(document: NodeRef, document_url: &str) -> Self {
        Self::new_with_character_set(document, document_url, "UTF-8")
    }

    pub(crate) fn new_with_character_set(
        document: NodeRef,
        document_url: &str,
        character_set: &str,
    ) -> Self {
        let module_loader = Rc::new(super::module_loader::WebModuleLoader::new());
        let host = Rc::new(RefCell::new(HostState::new(
            document,
            document_url,
            character_set,
            Rc::clone(&module_loader),
        )));
        let context = Box::new(
            Context::new(HostBridge::Document(Rc::downgrade(&host)))
                .expect("the V8 document realm can be initialized"),
        );
        let total_script_bytes = host.borrow().script_bytes.clone();
        Self {
            context: Some(context),
            host,
            total_script_bytes,
            initialized: false,
            prefer_timer_task: true,
            last_heap_sample: None,
            frames: Some(Default::default()),
        }
    }

    pub fn execute_initial(&mut self, scripts: &[ScriptInput]) -> ScriptOutcome {
        self.execute_initial_with_loader(scripts, None)
    }

    pub(crate) fn execute_initial_with_loader(
        &mut self,
        scripts: &[ScriptInput],
        dynamic_script_loader: Option<&mut DynamicScriptLoader<'_>>,
    ) -> ScriptOutcome {
        self.execute_initial_impl(scripts, dynamic_script_loader, false, true)
    }

    fn execute_initial_impl(
        &mut self,
        scripts: &[ScriptInput],
        dynamic_script_loader: Option<&mut DynamicScriptLoader<'_>>,
        defer_dynamic_scripts: bool,
        request_document_lifecycle: bool,
    ) -> ScriptOutcome {
        if self.initialized {
            return lifecycle_error("the document's initial scripts have already executed");
        }
        self.initialized = true;
        let Some(context) = self.context.as_deref_mut() else {
            return inactive_runtime_outcome();
        };
        let host = Rc::clone(&self.host);
        let mut dynamic_script_loader = dynamic_script_loader;
        let result = catch_unwind(AssertUnwindSafe(|| {
            execute_inner(
                scripts,
                context,
                &host,
                &self.total_script_bytes,
                &mut dynamic_script_loader,
                defer_dynamic_scripts,
                request_document_lifecycle,
            )
        }));
        self.finish_guarded_run(result)
    }

    pub(crate) fn execute_initial_deferred(
        &mut self,
        scripts: &[ScriptInput],
        module_loader: Option<&mut DynamicScriptLoader<'_>>,
    ) -> ScriptOutcome {
        self.execute_initial_impl(scripts, module_loader, true, true)
    }

    /// Executes newly available classic scripts as one event-loop task in this document's realm.
    pub fn execute_additional_with_loader(
        &mut self,
        scripts: &[ScriptInput],
        dynamic_script_loader: Option<&mut DynamicScriptLoader<'_>>,
    ) -> ScriptOutcome {
        if !self.initialized {
            return lifecycle_error("the document's initial scripts have not executed");
        }
        let Some(context) = self.context.as_deref_mut() else {
            return inactive_runtime_outcome();
        };
        let host = Rc::clone(&self.host);
        let mut dynamic_script_loader = dynamic_script_loader;
        let result = catch_unwind(AssertUnwindSafe(|| {
            execute_additional_inner(
                scripts,
                context,
                &host,
                &self.total_script_bytes,
                &mut dynamic_script_loader,
            )
        }));
        self.finish_guarded_run(result)
    }

    pub fn is_active(&self) -> bool {
        self.context.is_some()
    }

    pub fn set_document_cookie_header(&mut self, cookie_header: &str) {
        self.host
            .borrow_mut()
            .replace_cookies_from_header(cookie_header);
    }

    pub(crate) fn set_document_stylesheets(
        &mut self,
        stylesheets: &[crate::engine::css::StylesheetSource],
    ) {
        self.host
            .borrow_mut()
            .replace_document_stylesheets(stylesheets);
    }

    pub fn replace_cookie_snapshot(&mut self, version: u64, cookie_header: &str) {
        self.host
            .borrow_mut()
            .replace_cookie_snapshot(version, cookie_header);
    }

    pub fn replace_storage_snapshot(
        &mut self,
        area: crate::storage::StorageAreaKind,
        snapshot: crate::storage::StorageAreaSnapshot,
    ) -> Result<(), crate::storage::StorageError> {
        self.host
            .borrow_mut()
            .replace_storage_snapshot(area, snapshot)
    }

    pub fn set_document_state(
        &mut self,
        cookie_version: u64,
        cookie_header: &str,
        local_storage: crate::storage::StorageAreaSnapshot,
        session_storage: crate::storage::StorageAreaSnapshot,
    ) -> Result<(), crate::storage::StorageError> {
        let mut host = self.host.borrow_mut();
        host.replace_cookie_snapshot(cookie_version, cookie_header);
        host.replace_storage_snapshots(local_storage, session_storage)
    }

    /// Enables bounded native bridge timing for diagnostics produced by subsequent tasks.
    pub fn set_host_call_profiling(&mut self, enabled: bool) {
        self.host
            .borrow_mut()
            .host_call_profile
            .set_enabled(enabled);
    }

    /// Dispatches one browser-normalized native event as a bounded task in this realm.
    pub fn dispatch_user_input(&mut self, event: UserInputEvent) -> UserInputResult {
        if !self.initialized {
            return UserInputResult {
                outcome: lifecycle_error("the document's initial scripts have not executed"),
                default_allowed: false,
                rejected_text: None,
            };
        }
        let Some(context) = self.context.as_deref_mut() else {
            return UserInputResult {
                outcome: inactive_runtime_outcome(),
                default_allowed: false,
                rejected_text: None,
            };
        };
        let host = Rc::clone(&self.host);
        let result = catch_unwind(AssertUnwindSafe(|| {
            super::user_events::dispatch(context, &host, event)
        }));
        match result {
            Ok(mut result) => {
                result.outcome = self.finish_guarded_run(Ok(result.outcome));
                result
            }
            Err(payload) => UserInputResult {
                outcome: self.finish_guarded_run(Err(payload)),
                default_allowed: false,
                rejected_text: None,
            },
        }
    }
}

#[cfg(test)]
mod tests;
