//! Scheduling and outcome ownership for the related child document realms.
use super::*;
pub(super) mod documents;
mod images;
mod navigation;
mod resources;
mod script_fetches;
mod streaming;
mod styles;
use crate::engine::script::engine::frames::FrameNavigation;
use documents::FrameDocument;
use images::FrameImages;
use resources::FrameFetch;

pub(crate) struct FramePaintSnapshot {
    pub element: NodeId,
    pub document: NodeId,
    pub dom: NodeRef,
    pub url: String,
    pub stylesheets: Vec<crate::engine::css::StylesheetSource>,
    pub media_environment: crate::engine::MediaEnvironment,
    pub quirks_mode: bool,
    pub images: HashMap<String, crate::engine::DecodedImage>,
    pub children: Vec<FramePaintSnapshot>,
}

#[derive(Default)]
pub(super) struct ChildRuntimes {
    children: HashMap<NodeId, ScriptRuntime>,
    prefer_child: bool,
    last_child: Option<NodeId>,
    navigations: std::collections::VecDeque<FrameNavigation>,
    documents: HashMap<NodeId, FrameDocument>,
    fetches: HashMap<u32, FrameFetch>,
    prefer_message: bool,
    styles: HashMap<NodeId, styles::Sheets>,
    images: HashMap<NodeId, FrameImages>,
    pending: ScriptOutcome,
}

impl ScriptRuntime {
    pub(crate) fn frame_paint_snapshots(&mut self) -> Vec<FramePaintSnapshot> {
        self.frame_paint_snapshots_bounded(0)
    }

    fn frame_paint_snapshots_bounded(&mut self, depth: usize) -> Vec<FramePaintSnapshot> {
        if depth >= 8 {
            return Vec::new();
        }
        self.sync_child_runtimes();
        let document = self.host.borrow().document.id();
        let elements = self
            .context
            .as_ref()
            .map(|context| context.child_frame_elements(document))
            .unwrap_or_default();
        let Some(frames) = self.frames.as_mut() else {
            return Vec::new();
        };
        elements
            .into_iter()
            .filter_map(|(element, id)| {
                let child = frames.children.get_mut(&id)?;
                let snapshot = {
                    let host = child.host.borrow();
                    FramePaintSnapshot {
                        element,
                        document: id,
                        dom: host.document.clone(),
                        url: host.script_base_url(),
                        stylesheets: host.stylesheet_sources.clone(),
                        media_environment: host.media_environment,
                        quirks_mode: host.quirks_mode,
                        images: frames
                            .images
                            .get(&id)
                            .map(|images| images.decoded.clone())
                            .unwrap_or_default(),
                        children: Vec::new(),
                    }
                };
                Some(FramePaintSnapshot {
                    children: child.frame_paint_snapshots_bounded(depth + 1),
                    ..snapshot
                })
            })
            .collect()
    }

    pub(crate) fn dispatch_frame_input(
        &mut self,
        document: NodeId,
        event: UserInputEvent,
    ) -> Option<UserInputResult> {
        self.sync_child_runtimes();
        let child = self.frames.as_mut()?.children.get_mut(&document)?;
        let mut result = child.dispatch_user_input(event);
        result.outcome = self.collect_frame_result(document, result.outcome);
        Some(result)
    }
    pub(super) fn advance_message_task(&mut self) -> Option<ScriptOutcome> {
        let frames = self.frames.as_mut()?;
        frames.prefer_message = !frames.prefer_message;
        if !frames.prefer_message {
            return None;
        }
        let context = self.context.as_deref_mut()?;
        if !context.has_message_task() {
            return None;
        }
        let mut outcome = ScriptOutcome::default();
        if let Err(error) = context.deliver_message() {
            outcome.errors.push(format!("window message: {error}"));
        }
        Some(outcome)
    }

    pub(super) fn child_for_fetch(&mut self, id: u32) -> Option<&mut ScriptRuntime> {
        self.sync_child_runtimes();
        let owner = self.host.borrow().fetch_identifiers.borrow().owner(id)?;
        self.frames.as_mut()?.children.get_mut(&owner)
    }

    pub(super) fn child_for_worker(&mut self, id: u32) -> Option<&mut ScriptRuntime> {
        let owner = self.host.borrow().worker_identifiers.borrow().owner(id)?;
        self.frames.as_mut()?.children.get_mut(&owner)
    }

    pub(super) fn sync_child_runtimes(&mut self) {
        let (Some(frames), Some(context)) = (&mut self.frames, self.context.as_deref_mut()) else {
            return;
        };
        let active = context.child_documents();
        frames.documents.retain(|id, _| active.contains(id));
        frames.styles.retain(|id, _| active.contains(id));
        frames.images.retain(|id, _| active.contains(id));
        for navigation in context.frame_navigations() {
            frames
                .navigations
                .retain(|old| old.element != navigation.element);
            frames.navigations.push_back(navigation);
        }
        frames.fetches.retain(|id, fetch| {
            if fetch.current(context, &active) {
                return true;
            }
            let mut host = self.host.borrow_mut();
            host.fetch_identifiers.borrow_mut().finish(*id);
            host.pending_fetch_actions
                .push(ScriptFetchAction::Abort { id: *id });
            false
        });
        frames.children.retain(|id, _| {
            if active.contains(id) {
                return true;
            }
            let mut host = self.host.borrow_mut();
            let ids = host.fetch_identifiers.borrow_mut().cancel_document(*id);
            host.pending_fetch_actions
                .extend(ids.into_iter().map(|id| ScriptFetchAction::Abort { id }));
            let workers = host.worker_identifiers.borrow_mut().cancel_document(*id);
            host.pending_worker_actions.extend(
                workers
                    .into_iter()
                    .map(|id| ScriptWorkerAction::Terminate { id }),
            );
            false
        });
        let known = frames.children.keys().copied().collect();
        match context.new_child_contexts(&known) {
            Ok(children) => {
                for (id, host, context) in children {
                    let total_script_bytes = Rc::clone(&host.borrow().script_bytes);
                    frames.children.insert(
                        id,
                        Self {
                            context: Some(context),
                            host,
                            total_script_bytes,
                            initialized: true,
                            prefer_timer_task: true,
                            last_heap_sample: None,
                            frames: None,
                        },
                    );
                }
            }
            Err(error) => self
                .host
                .borrow_mut()
                .diagnose(format!("iframe task initialization: {error}")),
        }
        let pending = context.pending_frame_parents();
        let mut root = self.host.borrow_mut();
        root.document_load.child_documents_pending = pending.contains(&root.document.id());
        for (id, child) in &mut frames.children {
            child
                .host
                .borrow_mut()
                .document_load
                .child_documents_pending = pending.contains(id);
        }
    }

    pub(super) fn child_timer_delay(&mut self) -> Option<Duration> {
        let frames = self.frames.as_mut()?;
        if !frames.navigations.is_empty()
            || frames.styles.values().any(|sheets| sheets.dirty)
            || frames.documents.values().any(FrameDocument::runnable)
            || self
                .context
                .as_ref()
                .is_some_and(|context| context.frame_load(false).is_some())
        {
            return Some(Duration::ZERO);
        }
        frames
            .children
            .values_mut()
            .filter_map(|child| {
                if child.has_ready_dynamic_scripts() {
                    Some(Duration::ZERO)
                } else {
                    child.next_timer_delay()
                }
            })
            .min()
    }

    pub(super) fn elapse_child_time(&mut self, advance: Duration) {
        self.sync_child_runtimes();
        if let Some(frames) = &mut self.frames {
            for child in frames.children.values_mut() {
                child.elapse_time(advance);
            }
        }
    }

    pub(super) fn advance_child_task(&mut self) -> Option<ScriptOutcome> {
        if self.frames.as_ref()?.navigations.front().is_some() {
            return Some(self.advance_frame_navigation());
        }
        if let Some((parent, target)) = self.context.as_ref()?.frame_load(true) {
            let event = UserInputEvent::Simple {
                target,
                event_type: "load",
                bubbles: false,
                cancelable: false,
            };
            if parent == self.host.borrow().document.id() {
                return Some(self.dispatch_user_input(event).outcome);
            }
            let outcome = self
                .frames
                .as_mut()?
                .children
                .get_mut(&parent)?
                .dispatch_user_input(event)
                .outcome;
            return Some(self.collect_frame_result(parent, outcome));
        }
        let frames = self.frames.as_mut()?;
        frames.prefer_child = !frames.prefer_child;
        if !frames.prefer_child {
            return None;
        }
        let mut ready: Vec<_> = frames
            .children
            .iter_mut()
            .filter_map(|(id, child)| {
                (child.next_timer_delay() == Some(Duration::ZERO)
                    || child.has_ready_dynamic_scripts()
                    || frames
                        .documents
                        .get(id)
                        .is_some_and(FrameDocument::runnable))
                .then_some(*id)
            })
            .collect();
        ready.sort();
        let id = ready
            .iter()
            .copied()
            .find(|id| Some(*id) > frames.last_child)
            .or_else(|| ready.first().copied())?;
        frames.last_child = Some(id);
        let child = frames.children.get_mut(&id)?;
        let outcome = if let Some(document) = frames
            .documents
            .get_mut(&id)
            .filter(|document| document.runnable())
        {
            document.advance(child)
        } else {
            child.advance_time(Duration::ZERO, 1)
        };
        if let Err(error) = self.restart_frame_encoding(id) {
            self.host.borrow_mut().diagnose(error);
        }
        Some(self.collect_frame_result(id, outcome))
    }

    pub(super) fn collect_child_outcomes(&mut self, mut outcome: ScriptOutcome) -> ScriptOutcome {
        self.sync_child_runtimes();
        self.start_frame_styles();
        self.start_frame_images();
        self.start_frame_resources();
        if let Some(frames) = &mut self.frames {
            documents::append(&mut outcome, std::mem::take(&mut frames.pending));
        }
        let states: Vec<_> = self
            .frames
            .as_mut()
            .into_iter()
            .flat_map(|frames| frames.children.iter_mut())
            .map(|(id, child)| (*id, finish_host(ScriptOutcome::default(), &child.host)))
            .collect();
        for (id, state) in states {
            let state = self.collect_frame_result(id, state);
            documents::append(&mut outcome, state);
        }
        outcome
            .fetch_actions
            .append(&mut self.host.borrow_mut().pending_fetch_actions);
        outcome
            .websocket_actions
            .append(&mut self.host.borrow_mut().pending_websocket_actions);
        outcome
            .database_actions
            .append(&mut self.host.borrow_mut().pending_database_actions);
        outcome
            .worker_actions
            .append(&mut self.host.borrow_mut().pending_worker_actions);
        outcome
    }

    pub(super) fn collect_frame_result(
        &mut self,
        id: NodeId,
        mut outcome: ScriptOutcome,
    ) -> ScriptOutcome {
        // These browser-owned effects have no child-document wire identity yet. Never
        // apply a child's cookie/storage/media/fullscreen action to its parent's identity.
        outcome.cookie_updates.clear();
        outcome.storage_updates.clear();
        outcome.storage_event_receipts.clear();
        outcome.media_actions.clear();
        outcome.fullscreen_actions.clear();
        if outcome.navigation_options.post.is_some()
            || !matches!(outcome.navigation_options.target.as_str(), "" | "_self")
        {
            outcome.navigation_url = None;
            outcome
                .diagnostics
                .push("Embedded form POST and targeted navigation are not supported yet".into());
        }
        if let Some(url) = outcome.navigation_url.take()
            && let Some(navigation) = self
                .context
                .as_ref()
                .and_then(|context| context.request_frame_navigation(id, url))
            && let Some(frames) = &mut self.frames
        {
            frames
                .navigations
                .retain(|old| old.element != navigation.element);
            frames.navigations.push_back(navigation);
        }
        outcome.viewport_scroll_y = None;
        outcome.history_actions.clear();
        outcome
    }
}
