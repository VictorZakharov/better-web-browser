//! CSSOM state belongs to an owner/occurrence, never a shared downloaded URL.
use super::{Node, NodeRef};
use crate::engine::css::imports::SheetOverride;

#[derive(Debug, Default)]
pub(super) struct SheetState {
    signature: String,
    disabled: Option<bool>,
    overrides: Vec<SheetOverride>,
    preferred: String,
    scanned_version: Option<u64>,
    generation: u32,
}

impl Node {
    fn sheet_state(&self) -> &std::cell::RefCell<SheetState> {
        self.sheet_state.get_or_init(Default::default)
    }
    pub(crate) fn sheet_generation(&self) -> u32 {
        self.sheet_state().borrow().generation
    }
    pub(crate) fn stylesheet_subtree_removed(node: &NodeRef) {
        for node in
            Self::descendants(node).filter(|n| matches!(n.tag_name(), Some("style" | "link")))
        {
            if let Some(state) = node.sheet_state.get() {
                let mut state = state.borrow_mut();
                state.generation = state.generation.wrapping_add(1);
                state.overrides.clear();
                state.disabled = None;
                state.signature.clear();
            }
        }
    }
    fn sheet_signature(&self) -> String {
        format!(
            "{}\0{}\0{}\0{}\0{}\0{}\0{}",
            self.attr("href").unwrap_or_default(),
            self.attr("rel").unwrap_or_default(),
            self.attr("title").unwrap_or_default(),
            self.attr("type").unwrap_or_default(),
            self.attr("media").unwrap_or_default(),
            self.attr("disabled").is_some(),
            self.child_list_version().max(
                self.children
                    .borrow()
                    .iter()
                    .map(|n| n.subtree_mutation_version())
                    .max()
                    .unwrap_or(0)
            )
        )
    }

    pub(crate) fn sheet_overrides(&self) -> Vec<SheetOverride> {
        let state = self.sheet_state().borrow();
        if state.signature == self.sheet_signature() {
            state.overrides.clone()
        } else {
            Vec::new()
        }
    }

    pub(crate) fn set_sheet_overrides(&self, overrides: Vec<SheetOverride>) {
        let signature = self.sheet_signature();
        let mut state = self.sheet_state().borrow_mut();
        if state.signature != signature {
            state.disabled = None;
            state.signature = signature;
        }
        state.overrides = overrides;
        drop(state);
        self.mark_mutated();
    }

    pub(crate) fn set_sheet_disabled(&self, disabled: bool) {
        let signature = self.sheet_signature();
        let mut state = self.sheet_state().borrow_mut();
        if state.signature != signature {
            state.overrides.clear();
        }
        state.signature = signature;
        state.disabled = Some(disabled);
        drop(state);
        self.mark_mutated();
    }

    pub(crate) fn sheet_disabled(node: &NodeRef) -> bool {
        let root = Self::tree_root(node);
        let preferred = Self::preferred_sheet_set(&root);
        let state = node.sheet_state().borrow();
        if state.signature == node.sheet_signature()
            && let Some(disabled) = state.disabled
        {
            return disabled;
        }
        if node.attr("disabled").is_some() {
            return true;
        }
        let title = node.attr("title").unwrap_or_default();
        matches!(root.data, super::NodeData::Document) && !title.is_empty() && title != preferred
    }

    pub(crate) fn stylesheet_subtree_inserted(node: &NodeRef) {
        if !Self::descendants(node).any(|n| {
            matches!(n.tag_name(), Some("style" | "link"))
                && n.attr("title").is_some_and(|s| !s.is_empty())
        }) {
            return;
        }
        let root = Self::tree_root(node);
        if !matches!(root.data, super::NodeData::Document) {
            return;
        }
        if root.sheet_state().borrow().preferred.is_empty() {
            // Observe association at insertion, before a later insertBefore changes tree order.
            Self::preferred_sheet_set(&root);
        }
    }

    pub(crate) fn preferred_sheet_set(root: &NodeRef) -> String {
        let mut state = root.sheet_state().borrow_mut();
        if state.preferred.is_empty()
            && state.scanned_version != Some(root.document_mutation_version())
        {
            state.scanned_version = Some(root.document_mutation_version());
            for node in Self::descendants(root) {
                if !matches!(node.tag_name(), Some("style" | "link"))
                    || node.attr("disabled").is_some()
                {
                    continue;
                }
                let rel = node.attr("rel").unwrap_or_default();
                if node.tag_name() == Some("link")
                    && (!rel
                        .split_ascii_whitespace()
                        .any(|s| s.eq_ignore_ascii_case("stylesheet"))
                        || rel
                            .split_ascii_whitespace()
                            .any(|s| s.eq_ignore_ascii_case("alternate")))
                {
                    continue;
                }
                if node
                    .attr("type")
                    .is_some_and(|s| !s.trim().is_empty() && !s.eq_ignore_ascii_case("text/css"))
                {
                    continue;
                }
                let title = node.attr("title").unwrap_or_default();
                if !title.is_empty() {
                    state.preferred = title;
                    break;
                }
            }
        }
        state.preferred.clone()
    }
}
