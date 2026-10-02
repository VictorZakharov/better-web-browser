//! DOM mutation admission, invalidation, and animation discovery metadata.
use super::*;

impl HostState {
    pub(in crate::engine::script) fn record_mutation(
        &mut self,
        target: Option<&NodeRef>,
        kind: MutationKind<'_>,
    ) {
        let requires_render = target.is_some_and(|target| self.mutation_requires_render(target));
        self.record_mutation_with_render(target, kind, requires_render);
    }

    pub(in crate::engine::script) fn record_mutation_with_render(
        &mut self,
        target: Option<&NodeRef>,
        kind: MutationKind<'_>,
        requires_render: bool,
    ) {
        self.mutation_count += 1;
        self.task_mutations.record(kind);
        if let Some(target) = target
            && matches!(kind, MutationKind::Attribute("style"))
        {
            self.inline_transitions.style_changed(target);
        }
        self.invalidate_style_rules_for_mutation(target, kind);
        if requires_render {
            self.css_animation_revision = self.css_animation_revision.wrapping_add(1);
            self.pending_invalidation
                .record(&self.document, target, kind);
            self.pending_layout_invalidation
                .record(&self.document, target, kind);
            self.timers.request_render();
        }
    }

    pub(in crate::engine::script) fn begin_task(&mut self) {
        self.idle_callbacks.interrupt();
        self.begin_idle_task();
    }

    pub(in crate::engine::script) fn begin_idle_task(&mut self) {
        self.task_mutations.reset();
        self.task_started = Some(Instant::now());
    }

    pub(in crate::engine::script) fn invalidate_previous_parent(
        &mut self,
        target: &NodeRef,
        moved: &NodeRef,
        kind: MutationKind<'_>,
    ) {
        // Pre-insertion removes the child from its old parent. Only a connected old parent
        // affects rendering; detached staging fragments must not widen the dirty root set.
        // Conversely, moving into a detached tree must still render the connected removal.
        // https://dom.spec.whatwg.org/#concept-node-insert
        if !self.mutation_requires_render(target) {
            return;
        }
        self.invalidate_style_rules_for_mutation(Some(target), kind);
        self.pending_invalidation
            .record(&self.document, Some(target), kind);
        self.pending_layout_invalidation
            .record(&self.document, Some(target), kind);
        if !self.is_connected(moved) {
            self.record_removed_subtree(moved);
        }
        self.timers.request_render();
    }

    pub(in crate::engine::script) fn record_removed_subtree(&mut self, root: &NodeRef) {
        self.pending_invalidation.record_removed_subtree(root);
        self.pending_layout_invalidation
            .record_removed_subtree(root);
    }

    pub(in crate::engine::script) fn mutation_requires_render(&self, target: &NodeRef) -> bool {
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

    pub(in crate::engine::script) fn is_connected(&self, node: &NodeRef) -> bool {
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
