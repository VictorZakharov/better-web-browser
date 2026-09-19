//! Realm handles handed to the document task scheduler, never a second isolate/watchdog.
use super::*;
use crate::engine::dom::NodeId;
use crate::engine::script::host_state::HostState;
use std::collections::HashSet;

pub(in crate::engine::script) type ChildContext = (NodeId, Rc<RefCell<HostState>>, Box<Context>);

impl Context {
    pub(in crate::engine::script) fn fail_frame_navigation(
        &self,
        navigation: &super::super::frames::FrameNavigation,
    ) {
        self._frames.navigation_failed(navigation);
    }
    pub(in crate::engine::script) fn frame_ancestor_origins(
        &self,
        element: NodeId,
    ) -> Vec<crate::fetch::Origin> {
        self._frames.ancestor_origins(element)
    }
    pub(in crate::engine::script) fn refresh_code_generation_policy(&mut self) {
        self.agent
            .borrow_mut()
            .run(|isolate| {
                v8::scope!(let scope, isolate);
                let context = v8::Local::new(scope, &self.context);
                if let Some(host) = super::super::node_wrappers::host(context) {
                    let host = host.borrow();
                    context.set_allow_generation_from_strings(
                        !host.sandbox.scripts_blocked && host.policy.allows_eval(),
                    );
                }
                Ok(())
            })
            .expect("policy update does not execute author code");
    }
    pub(in crate::engine::script) fn request_frame_navigation(
        &self,
        document: NodeId,
        url: String,
    ) -> Option<super::super::frames::FrameNavigation> {
        self._frames.request_navigation(document, url)
    }

    pub(in crate::engine::script) fn pending_frame_parents(&self) -> HashSet<NodeId> {
        self._frames.pending_parents()
    }
    pub(in crate::engine::script) fn frame_load(
        &self,
        take: bool,
    ) -> Option<(NodeId, crate::engine::dom::NodeRef)> {
        self._frames.ready_load(take)
    }
    pub(in crate::engine::script) fn frame_navigation_current(
        &self,
        navigation: &super::super::frames::FrameNavigation,
    ) -> bool {
        self._frames
            .navigation_current(navigation.element, navigation.document, navigation.epoch)
    }
    pub(in crate::engine::script) fn frame_navigations(
        &self,
    ) -> Vec<super::super::frames::FrameNavigation> {
        self._frames.navigations()
    }

    pub(in crate::engine::script) fn replace_frame(
        &mut self,
        element: NodeId,
        document: crate::engine::dom::NodeRef,
        url: String,
    ) -> JsResult<Option<NodeId>> {
        self.agent.borrow_mut().run(|isolate| {
            v8::scope!(let scope, isolate);
            let root = v8::Local::new(scope, &self.context);
            let scope = &mut v8::ContextScope::new(scope, root);
            self._frames
                .replace(scope, element, document, url)
                .map(Some)
                .ok_or_else(|| allocation_error("replace child document"))
        })
    }
    pub(in crate::engine::script) fn install_window_bindings(&mut self) -> JsResult<()> {
        let context = self.context.clone();
        self.agent.borrow_mut().run(|isolate| {
            v8::scope!(let scope, isolate);
            let local = v8::Local::new(scope, context);
            let scope = &mut v8::ContextScope::new(scope, local);
            super::super::messaging::install(scope)
                .and_then(|()| super::super::window_access::install(scope))
                .ok_or_else(|| allocation_error("window messaging bindings"))
        })
    }

    pub(in crate::engine::script) fn has_message_task(&self) -> bool {
        self._frames.messages.pending() || self._frames.ports.pending()
    }

    pub(in crate::engine::script) fn deliver_message(&mut self) -> JsResult<()> {
        let context = self.context.clone();
        self.agent.borrow_mut().run(|isolate| {
            v8::scope!(let scope, isolate);
            let local = v8::Local::new(scope, context);
            let scope = &mut v8::ContextScope::new(scope, local);
            v8::tc_scope!(let tc, scope);
            let prefer_port = !self._frames.prefer_port_message.get();
            self._frames.prefer_port_message.set(prefer_port);
            let result = if self._frames.ports.pending()
                && (prefer_port || !self._frames.messages.pending())
            {
                self._frames.ports.deliver(tc)
            } else {
                self._frames.messages.deliver(tc)
            };
            result.ok_or_else(|| caught_error(tc, "posted-message delivery"))
        })
    }

    pub(in crate::engine::script) fn child_documents(&self) -> HashSet<NodeId> {
        self._frames
            .snapshot()
            .into_iter()
            .map(|(id, ..)| id)
            .collect()
    }

    pub(in crate::engine::script) fn new_child_contexts(
        &mut self,
        known: &HashSet<NodeId>,
    ) -> JsResult<Vec<ChildContext>> {
        let snapshots = self._frames.snapshot();
        self.agent.borrow_mut().run(|isolate| {
            v8::scope!(let scope, isolate);
            let mut contexts = Vec::new();
            for (id, context, host, storage_dispatch) in snapshots {
                if known.contains(&id) {
                    continue;
                }
                let local = v8::Local::new(scope, &context);
                let imports = local
                    .get_slot::<super::super::dynamic_imports::Imports>()
                    .ok_or_else(|| allocation_error("child module registry"))?;
                let context = Box::new(Self {
                    context,
                    private_hooks: HashMap::from([(
                        "__dispatchStorageEvent".into(),
                        storage_dispatch,
                    )]),
                    imports,
                    _frames: Rc::clone(&self._frames),
                    next_module_promise: 1,
                    module_promises: HashMap::new(),
                    agent: Rc::clone(&self.agent),
                });
                contexts.push((id, host, context));
            }
            Ok(contexts)
        })
    }
}
