//! Authoritative form-control state: live values versus authored defaults.
//!
//! `input`/`textarea` keep a dirty value flag: pristine controls mirror their
//! default (the `value` attribute, or child text for `textarea`), while dirty
//! controls ignore later default changes. `option` keeps selectedness plus a
//! dirtiness flag with the same split. `output` keeps its default-value
//! override. Everything scripted, laid out, painted, or submitted reads this
//! state; JS wrappers delegate to it instead of keeping a second store.
//!
//! State is seeded lazily from current attributes on first control access, so
//! parser, script, and host construction paths share one initialization.

use super::control_values::{
    InputValueMode, canonical_input_state, input_value_mode, is_strict_float, normalize_newlines,
};
use super::{Node, NodeRef};

mod types;
pub(crate) use types::ControlState;

impl Node {
    fn control_cell(&self) -> Option<std::cell::Ref<'_, Option<Box<ControlState>>>> {
        self.element().map(|element| element.control_state.borrow())
    }

    /// Snapshots control state, seeding from attributes on first use.
    pub(crate) fn control_snapshot(&self) -> Option<ControlState> {
        if !self.is_control() {
            return None;
        }
        if self.control_cell()?.is_none() {
            self.seed_control_state();
        }
        let cell = self.control_cell()?;
        cell.as_ref().map(|boxed| (**boxed).clone())
    }

    /// Seeds live state from current attributes; idempotent.
    fn seed_control_state(&self) {
        let Some(element) = self.element() else {
            return;
        };
        if element.control_state.borrow().is_some() {
            return;
        }
        let tag = self.tag_name().unwrap_or_default().to_string();
        let mut state = ControlState::default();
        if tag == "input" {
            let input_type = canonical_input_state(&self.attr("type").unwrap_or_default());
            state.type_seen = Some(input_type.clone());
            if input_value_mode(&input_type) == InputValueMode::Value {
                state.value =
                    Some(self.sanitize_control_value(
                        &input_type,
                        &self.attr("value").unwrap_or_default(),
                    ));
            }
        } else if tag == "option" {
            state.selectedness = self.attr("selected").is_some();
        }
        element.control_state.borrow_mut().replace(Box::new(state));
    }

    /// True for elements that own control state.
    pub(crate) fn is_control(&self) -> bool {
        matches!(
            self.tag_name(),
            Some("input" | "textarea" | "select" | "option" | "output" | "button" | "fieldset")
        )
    }

    pub(crate) fn control_state_snapshot(&self) -> ControlState {
        self.control_snapshot().unwrap_or_default()
    }

    /// Live value for value-mode inputs; pristine controls mirror the default.
    pub(crate) fn input_value(&self) -> String {
        let state = self.control_state_snapshot();
        let input_type = self.input_state_name();
        match input_value_mode(&input_type) {
            InputValueMode::DefaultOn => self.attr("value").unwrap_or_else(|| "on".into()),
            InputValueMode::Filename => state
                .file_names
                .first()
                .map_or_else(String::new, |name| format!("C:\\fakepath\\{name}")),
            InputValueMode::Default | InputValueMode::ButtonDefault => {
                self.sanitize_control_value(&input_type, &self.attr("value").unwrap_or_default())
            }
            InputValueMode::Value => state.value.unwrap_or_else(|| {
                self.sanitize_control_value(&input_type, &self.attr("value").unwrap_or_default())
            }),
        }
    }

    /// Preserve unconvertible user text visually without exposing a stale API value.
    pub(crate) fn input_display_value(&self) -> String {
        self.control_state_snapshot()
            .editing
            .unwrap_or_else(|| self.input_value())
    }

    /// Programmatic value write. Value-mode inputs sanitize into live state
    /// with dirty set; default-mode inputs (`hidden`, buttons, checkable)
    /// write the `value` attribute, which *is* their value.
    /// Returns true when the IDL value actually changed.
    pub(crate) fn set_input_value(&self, value: &str) -> bool {
        let mode = input_value_mode(&self.input_state_name());
        if mode == InputValueMode::Filename {
            if !value.is_empty() {
                return false;
            }
            let before = !self.control_state_snapshot().file_names.is_empty();
            self.set_input_files(Vec::new());
            return before;
        }
        if mode != InputValueMode::Value {
            let before = self.input_value();
            let sanitized = self.sanitize_control_value(&self.input_state_name(), value);
            self.update_control_state_tracked(|state| {
                state.dirty = false;
                state.user_edited = false;
                state.editing = None;
                state.reported = false;
            });
            let _ = self.set_attr("value", &sanitized);
            return self.input_value() != before;
        }
        let sanitized = self.sanitize_control_value(&self.input_state_name(), value);
        self.store_input_value(&sanitized, false)
    }

    pub(crate) fn set_input_files(&self, names: Vec<String>) {
        if self.tag_name() != Some("input") || self.input_state_name() != "file" {
            return;
        }
        self.update_control_state_tracked(|state| {
            state.file_names = names;
            state.reported = false;
            state.user_validity = false;
        });
    }

    /// Stores a sanitized value-mode write; range values clamp to their
    /// bounds and range empties fall back to the default.
    fn store_input_value(&self, sanitized: &str, by_user: bool) -> bool {
        let mut changed = false;
        self.update_control_state_tracked(|state| {
            if state.value.as_deref() != Some(sanitized) {
                state.value = Some(sanitized.to_string());
                changed = true;
            }
            state.dirty = true;
            state.user_edited = by_user;
            state.editing = None;
            state.reported = false;
            if by_user {
                state.user_validity = true;
            }
        });
        changed
    }

    /// User edit: like a programmatic write but marks user-edited and retains
    /// unconvertible number text for `badInput` instead of discarding it.
    /// Returns true when live state actually changed.
    pub(crate) fn user_edit_input(&self, value: &str) -> bool {
        let input_type = self.input_state_name();
        if input_value_mode(&input_type) != InputValueMode::Value {
            return self.set_input_value(value);
        }
        if input_type == "number" && !value.is_empty() && !is_strict_float(value) {
            let mut changed = false;
            self.update_control_state_tracked(|state| {
                changed = state.editing.as_deref() != Some(value);
                state.editing = Some(value.to_string());
                state.value = Some(String::new());
                state.dirty = true;
                state.user_edited = true;
                state.user_validity = true;
                state.reported = false;
            });
            return changed;
        }
        let sanitized = self.sanitize_control_value(&input_type, value);
        self.store_input_value(&sanitized, true)
    }

    /// Lowercase input state name (`text` for missing/invalid types).
    pub(crate) fn input_state_name(&self) -> String {
        if self.tag_name() != Some("input") {
            return String::new();
        }
        canonical_input_state(
            &self
                .control_state_snapshot()
                .type_seen
                .or_else(|| self.attr("type"))
                .unwrap_or_default(),
        )
    }

    /// Attribute mutation hook for control-relevant attributes (null namespace).
    pub(in crate::engine::dom) fn control_attribute_changed(&self, name: &str) {
        let tag = self.tag_name();
        if tag == Some("input") && name == "value" {
            let snapshot = self.control_state_snapshot();
            if !snapshot.dirty {
                let live = self.sanitize_control_value(
                    &self.input_state_name(),
                    &self.attr("value").unwrap_or_default(),
                );
                self.update_control_state_tracked(|state| {
                    state.value = Some(live);
                });
            }
            return;
        }
        if tag == Some("input")
            && matches!(name, "min" | "max" | "step")
            && self.input_state_name() == "range"
        {
            let value = self.sanitize_control_value("range", &self.input_value());
            self.update_control_state_tracked(|state| state.value = Some(value));
        }
        if tag == Some("input") && name == "type" {
            self.apply_type_transition();
            return;
        }
        if tag == Some("option") && name == "selected" {
            self.apply_selected_attribute();
        }
    }

    pub(crate) fn input_default_value(&self) -> String {
        self.attr("value").unwrap_or_default()
    }

    /// Mutable control-state access with lazy seeding.
    pub(crate) fn update_control_state(&self, update: impl FnOnce(&mut ControlState)) {
        if !self.is_control() {
            return;
        }
        if self
            .element()
            .is_some_and(|element| element.control_state.borrow().is_none())
        {
            self.seed_control_state();
        }
        if let Some(element) = self.element()
            && let Some(state) = element.control_state.borrow_mut().as_mut()
        {
            update(state);
        }
    }

    /// Textarea raw value (un-normalized); pristine controls track children.
    pub(crate) fn textarea_raw(&self) -> String {
        let snapshot = self.control_state_snapshot();
        if let Some(raw) = snapshot.value {
            return raw;
        }
        self.text_content()
    }

    /// Textarea write (programmatic or user); sets dirty like input values.
    pub(crate) fn set_textarea_raw(&self, value: &str, by_user: bool) -> bool {
        let mut changed = false;
        self.update_control_state_tracked(|state| {
            if state.value.as_deref() != Some(value) {
                state.value = Some(value.to_string());
                changed = true;
            }
            state.dirty = true;
            state.user_edited = by_user;
            state.reported = false;
            if by_user {
                state.user_validity = true;
            }
        });
        changed
    }

    /// Textarea API value: raw with CRLF/CR normalized to LF.
    pub(crate) fn textarea_api_value(&self) -> String {
        normalize_newlines(&self.textarea_raw())
    }

    /// Child-list changes feed pristine textareas; dirty edits are kept.
    pub(in crate::engine::dom) fn textarea_children_changed(&self) {
        if self.tag_name() != Some("textarea") {
            return;
        }
        if self.control_state_snapshot().dirty {
            return;
        }
        let text = self.text_content();
        self.update_control_state_tracked(|state| {
            state.value = Some(text);
        });
    }

    /// Input reset algorithm: flags cleared, value follows the default again.
    pub(crate) fn reset_input(&self) {
        self.update_control_state_tracked(|state| {
            state.value = None;
            state.dirty = false;
            state.user_edited = false;
            state.user_validity = false;
            state.editing = None;
            state.file_names.clear();
            state.reported = false;
        });
        self.reset_checked();
    }

    /// Textarea reset: raw value returns to child text.
    pub(crate) fn reset_textarea(&self) {
        let text = self.text_content();
        self.update_control_state_tracked(|state| {
            state.value = Some(text);
            state.dirty = false;
            state.user_edited = false;
            state.user_validity = false;
            state.reported = false;
        });
    }

    /// Output reset: children become the default value, override cleared.
    pub(crate) fn reset_output(node: &NodeRef) {
        let default = node
            .control_state_snapshot()
            .default_override
            .clone()
            .unwrap_or_else(|| node.text_content());
        Self::set_text_content(node, &default);
        node.update_control_state_tracked(|state| {
            state.default_override = None;
        });
    }

    /// Clears user validity without touching values (select reset helper).
    pub(crate) fn clear_user_validity(&self) {
        self.update_control_state_tracked(|state| {
            state.user_validity = false;
            state.reported = false;
        });
    }

    /// Custom validity message write with newline normalization.
    pub(crate) fn set_custom_message(&self, message: &str) {
        let normalized = message.replace("\r\n", "\n").replace('\r', "\n");
        self.update_control_state_tracked(|state| {
            state.custom_message = normalized;
            state.reported = false;
        });
    }

    /// Insertion hook: options enforce single-select; called per subtree node.
    pub(in crate::engine::dom) fn control_subtree_inserted(root: &NodeRef) {
        for node in Node::descendants(root) {
            node.control_option_inserted();
        }
    }

    /// Child-list hook: textarea defaults track children; selects renormalize.
    pub(in crate::engine::dom) fn control_child_changed(parent: &NodeRef) {
        if parent.tag_name() == Some("textarea") {
            parent.textarea_children_changed();
        }
        let mut ancestor = Some(parent.clone());
        while let Some(node) = ancestor {
            if node.tag_name() == Some("select") {
                node.run_selectedness_setting();
                return;
            }
            if matches!(node.tag_name(), Some("form" | "html" | "body")) {
                return;
            }
            ancestor = node.parent();
        }
    }

    /// Propagates cloneable control state per the platform cloning steps.
    /// Custom messages and user validity never copy; option selectedness is
    /// attribute-driven on the copy.
    pub(crate) fn propagate_clone_state(&self, copy: &Node) {
        let snapshot = self.control_state_snapshot();
        copy.update_control_state_tracked(|state| {
            state.value = snapshot.value.clone();
            state.dirty = snapshot.dirty;
            state.user_edited = snapshot.user_edited;
            state.editing = snapshot.editing.clone();
        });
    }
}
