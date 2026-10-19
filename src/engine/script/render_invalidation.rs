//! Per-realm accumulation of the minimum conservative rendering invalidation.

use super::*;
use crate::engine::invalidation::{is_validation_attribute, validation_aggregation_roots};
use std::collections::BTreeSet;

#[derive(Default)]
pub(super) struct PendingInvalidation {
    roots: Vec<NodeRef>,
    impact: InvalidationImpact,
    rebuild_style_rules: bool,
    removed_nodes: BTreeSet<NodeId>,
    removed_global_dependency: bool,
    class_changes: Vec<crate::engine::invalidation::ClassChange>,
    /// Parents whose child lists changed and that no other mutation made a full root. They
    /// never cover other roots: the cascade may restyle only part of their subtrees.
    child_lists: Vec<NodeRef>,
}

impl PendingInvalidation {
    pub(super) fn record(
        &mut self,
        document: &NodeRef,
        target: Option<&NodeRef>,
        kind: MutationKind<'_>,
    ) {
        self.record_with_style_scope(document, target, kind, false);
    }

    /// Records changed class tokens on `target` for the cascade to bound, instead of the
    /// target's parent subtree. A proper ancestor root already restyles the element and every
    /// sibling, so the change adds nothing there.
    pub(super) fn record_class_change(
        &mut self,
        document: &NodeRef,
        target: &NodeRef,
        tokens: Vec<String>,
    ) {
        self.impact = self.impact.union(MutationKind::Attribute("class").impact());
        let covered = target.shadow_including_parent().is_some_and(|parent| {
            self.roots
                .iter()
                .any(|root| is_descendant_of(&parent, root))
        });
        if covered {
            return;
        }
        if self.class_changes.len() >= crate::engine::invalidation::MAX_INVALIDATION_ROOTS {
            let parent = target
                .shadow_including_parent()
                .unwrap_or_else(|| document.clone());
            self.extend(document, &parent);
            return;
        }
        crate::engine::invalidation::record_class_change(&mut self.class_changes, target, tokens);
    }

    pub(super) fn record_selector_independent_style(
        &mut self,
        document: &NodeRef,
        target: &NodeRef,
    ) {
        self.record_with_style_scope(
            document,
            Some(target),
            MutationKind::Attribute("style"),
            true,
        );
    }

    fn record_with_style_scope(
        &mut self,
        document: &NodeRef,
        target: Option<&NodeRef>,
        kind: MutationKind<'_>,
        selector_independent_style: bool,
    ) {
        let rebuild_rules = rebuilds_style_rules(target, kind);
        self.impact = self.impact.union(kind.impact());
        if rebuild_rules {
            self.impact = self.impact.union(MutationKind::Stylesheet.impact());
        }
        self.rebuild_style_rules |= rebuild_rules;
        let Some(target) = target else {
            return;
        };
        let root = match kind {
            // Private effect declarations leave authored attributes, sibling
            // selectors and tree membership unchanged. Recompute inheritance
            // below the target; geometry changes still request normal layout.
            MutationKind::StyleOverlay => target.clone(),
            MutationKind::Attribute("style") if selector_independent_style => target.clone(),
            // A stylesheet's text replacement keeps the old child-list scope;
            // rebuilding rules already clears local-content evidence above.
            MutationKind::TextChildren if rebuild_rules => target.clone(),
            MutationKind::Attribute(_)
            | MutationKind::CharacterData
            | MutationKind::TextChildren
            | MutationKind::State
            | MutationKind::PointerDesignation => target
                .shadow_including_parent()
                .unwrap_or_else(|| target.clone()),
            // Keep the DOM mutation's scope even when rules may need rebuilding. The style
            // consumer widens to the document only if the effective rule inputs changed.
            MutationKind::ChildList if !rebuild_rules => {
                self.extend_child_list(document, target);
                self.extend_validation_dependents(document, target, kind);
                return;
            }
            MutationKind::ChildList | MutationKind::Stylesheet => target.clone(),
            MutationKind::Viewport => document.clone(),
        };
        self.extend(document, &root);
        self.extend_validation_dependents(document, target, kind);
    }

    /// Adds form-owner and ancestor-fieldset roots when a mutation can flip
    /// validation selectors beyond the recorded target's own subtree (HTML
    /// `form`/`fieldset` `:valid` / `:invalid` aggregation). The helper
    /// returns an empty set for unrelated mutations, so ordinary edits keep
    /// their existing narrow roots. Moving a control records both sides:
    /// the new side here and the old side via `invalidate_previous_parent`.
    fn extend_validation_dependents(
        &mut self,
        document: &NodeRef,
        target: &NodeRef,
        kind: MutationKind<'_>,
    ) {
        let relevant = matches!(
            kind,
            MutationKind::State | MutationKind::ChildList | MutationKind::TextChildren
        ) || matches!(kind, MutationKind::Attribute(name) if is_validation_attribute(name));
        if !relevant {
            return;
        }
        for extra in validation_aggregation_roots(target) {
            self.extend(document, &extra);
        }
    }

    pub(super) fn extend(&mut self, document: &NodeRef, target: &NodeRef) {
        if self.roots.iter().any(|root| is_descendant_of(target, root)) {
            return;
        }
        self.roots.retain(|root| !is_descendant_of(root, target));
        self.child_lists
            .retain(|parent| !is_descendant_of(parent, target));
        self.roots.push(target.clone());
        self.limit(document);
    }

    /// Records that `parent`'s children changed, unless a full root already restyles it.
    fn extend_child_list(&mut self, document: &NodeRef, parent: &NodeRef) {
        if self.roots.iter().any(|root| is_descendant_of(parent, root))
            || self
                .child_lists
                .iter()
                .any(|known| known.id() == parent.id())
        {
            return;
        }
        self.child_lists.push(parent.clone());
        self.limit(document);
    }

    fn limit(&mut self, document: &NodeRef) {
        if self.roots.len() + self.child_lists.len()
            > crate::engine::invalidation::MAX_INVALIDATION_ROOTS
        {
            self.roots.clear();
            self.child_lists.clear();
            self.roots.push(document.clone());
        }
    }

    pub(super) fn record_removed_subtree(&mut self, root: &NodeRef) {
        for node in Node::shadow_including_descendants(root) {
            self.removed_nodes.insert(node.id());
            // SVG definitions, slot redistribution, base URLs, and top-layer content affect boxes outside
            // an otherwise hidden subtree. Retain this evidence before losing connectivity.
            self.removed_global_dependency |= node.is_fullscreen()
                || matches!(node.tag_name(), Some("base" | "slot"))
                || node
                    .namespace_uri()
                    .is_some_and(|ns| ns != "http://www.w3.org/1999/xhtml");
        }
    }

    pub(super) fn acknowledge_published_geometry(&mut self) {
        // The renderer has already laid out all content changes through this checkpoint.
        // Its geometry does not refresh the script snapshot's independent style cache:
        // retain dirty roots, removed styles, and rule-rebuild obligations for that cache.
        self.impact = self.impact.without_intrinsic_size();
    }

    pub(super) fn snapshot(&self, mutation_count: usize) -> RenderInvalidation {
        RenderInvalidation {
            roots: self
                .roots
                .iter()
                .chain(&self.child_lists)
                .map(|root| root.id())
                .collect(),
            impact: self.impact,
            mutation_count,
            rebuild_style_rules: self.rebuild_style_rules,
            removed_nodes: self.removed_nodes.iter().copied().collect(),
            removals_are_local: !self.removed_nodes.is_empty() && !self.removed_global_dependency,
            class_changes: self.class_changes.clone(),
            child_list_roots: self.child_lists.iter().map(|root| root.id()).collect(),
        }
    }

    pub(super) fn take(&mut self, mutation_count: usize) -> RenderInvalidation {
        let result = self.snapshot(mutation_count);
        *self = Self::default();
        result
    }
}

pub(super) fn rebuilds_style_rules(target: Option<&NodeRef>, kind: MutationKind<'_>) -> bool {
    matches!(kind, MutationKind::Stylesheet)
        || (matches!(
            kind,
            MutationKind::Attribute("href" | "rel" | "media" | "type" | "disabled" | "title")
        ) && target.is_some_and(|node| matches!(node.tag_name(), Some("style" | "link"))))
        || (matches!(
            kind,
            MutationKind::CharacterData | MutationKind::ChildList | MutationKind::TextChildren
        ) && target.is_some_and(is_in_style_element))
}

fn is_in_style_element(node: &NodeRef) -> bool {
    std::iter::successors(Some(node.clone()), |current| {
        current.shadow_including_parent()
    })
    .any(|current| current.tag_name() == Some("style"))
}

fn is_descendant_of(node: &NodeRef, ancestor: &NodeRef) -> bool {
    std::iter::successors(Some(node.clone()), |node| node.shadow_including_parent())
        .any(|node| node.id() == ancestor.id())
}

#[cfg(test)]
mod tests;
