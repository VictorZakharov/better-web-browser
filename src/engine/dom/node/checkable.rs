//! HTML checkedness is live state, separate from the checked content attribute.
use super::{Node, NodeId, NodeRef};

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct InputState {
    pub checked: bool,
    pub dirty: bool,
    pub indeterminate: bool,
}

impl Node {
    pub(crate) fn is_checkable(&self) -> bool {
        self.tag_name() == Some("input")
            && self.attr("type").is_some_and(|value| {
                value.eq_ignore_ascii_case("radio") || value.eq_ignore_ascii_case("checkbox")
            })
    }

    pub(crate) fn is_radio(&self) -> bool {
        self.tag_name() == Some("input")
            && self
                .attr("type")
                .is_some_and(|value| value.eq_ignore_ascii_case("radio"))
    }

    pub(crate) fn checked(&self) -> bool {
        self.element()
            .is_some_and(|element| element.input_state.get().checked)
    }

    pub(crate) fn indeterminate(&self) -> bool {
        self.element()
            .is_some_and(|element| element.input_state.get().indeterminate)
    }

    pub(crate) fn set_indeterminate(&self, value: bool) {
        if let Some(element) = self.element() {
            let mut state = element.input_state.get();
            if state.indeterminate != value {
                state.indeterminate = value;
                element.input_state.set(state);
                self.mark_mutated();
            }
        }
    }

    pub(crate) fn set_checked(&self, value: bool, dirty: bool) {
        if let Some(element) = self.element() {
            let mut state = element.input_state.get();
            state.dirty |= dirty;
            let changed = state.checked != value;
            state.checked = value;
            element.input_state.set(state);
            if changed {
                self.mark_mutated();
            }
            if value {
                self.enforce_radio_group();
            }
        }
    }

    pub(crate) fn reset_checked(&self) {
        if let Some(element) = self.element() {
            let mut state = element.input_state.get();
            state.dirty = false;
            element.input_state.set(state);
            self.set_checked(self.attr("checked").is_some(), false);
        }
    }

    pub(crate) fn form_owner(&self) -> Option<NodeId> {
        if let Some(id) = self.attr("form") {
            let root = self.ancestor_root()?;
            return Node::descendants(&root)
                .find(|node| node.attr("id").as_deref() == Some(id.as_str()))
                .filter(|node| node.tag_name() == Some("form"))
                .map(|node| node.id());
        }
        std::iter::successors(self.parent(), |node| node.parent())
            .find(|node| node.tag_name() == Some("form"))
            .map(|node| node.id())
    }

    fn ancestor_root(&self) -> Option<NodeRef> {
        std::iter::successors(self.parent(), |node| node.parent()).last()
    }

    pub(crate) fn radio_group(&self) -> Vec<NodeRef> {
        let Some(name) = self.attr("name").filter(|name| !name.is_empty()) else {
            return vec![];
        };
        if !self.is_radio() {
            return vec![];
        }
        let Some(root) = self.ancestor_root() else {
            return vec![];
        };
        let owner = self.form_owner();
        Node::descendants(&root)
            .filter(|node| {
                node.is_radio()
                    && node.attr("name").as_deref() == Some(name.as_str())
                    && node.form_owner() == owner
            })
            .collect()
    }

    fn enforce_radio_group(&self) {
        if !self.checked() || !self.is_radio() {
            return;
        }
        for other in self.radio_group() {
            if other.id() != self.id() {
                other.set_checked(false, false);
            }
        }
    }

    pub(in crate::engine::dom) fn checkable_attribute_changed(&self, name: &str) {
        if self.tag_name() != Some("input") {
            return;
        }
        if name == "checked" && self.element().is_some_and(|e| !e.input_state.get().dirty) {
            self.set_checked(self.attr("checked").is_some(), false);
        } else if matches!(name, "name" | "form" | "type") {
            self.enforce_radio_group();
        }
    }

    pub(in crate::engine::dom) fn checkable_subtree_inserted(root: &NodeRef) {
        for node in Node::descendants(root) {
            node.enforce_radio_group();
        }
    }
}
