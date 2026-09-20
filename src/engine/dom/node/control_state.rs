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
    InputValueMode, canonical_input_state, input_value_mode, is_valid_float, normalize_newlines,
    range_default, sanitize_input_value,
};
use super::{Node, NodeRef};

/// Live control state. Only the fields matching the element kind are used.
#[derive(Clone, Debug, Default)]
pub(crate) struct ControlState {
    /// Live value for text-like inputs (`None` = pristine, mirror default).
    pub value: Option<String>,
    /// Dirty value flag (input, textarea).
    pub dirty: bool,
    /// True when the last value change was a user edit (length constraints).
    pub user_edited: bool,
    /// Spec user-validity flag, set by user interaction, cleared by reset.
    pub user_validity: bool,
    /// Number inputs: unconvertible user-entered text (`badInput` source).
    pub editing: Option<String>,
    /// Option selectedness.
    pub selectedness: bool,
    /// Option dirtiness (selected-attribute changes stop applying).
    pub selected_dirty: bool,
    /// Output default-value override (`None` = follow descendant text).
    pub default_override: Option<String>,
    /// Custom validity message (newline-normalized on write).
    pub custom_message: String,
    /// Last scripted pattern verdict with the (pattern, values) it was
    /// computed from; selector matching trusts it only on exact deps.
    pub pattern_verdict: Option<(String, Vec<String>, bool)>,
    /// Last observed `type` attribute, for type-change transitions.
    pub type_seen: Option<String>,
}

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

    fn update_control(&self, update: impl FnOnce(&mut ControlState)) {
        self.update_control_state(update);
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
                state.value = Some(sanitize_input_value(
                    &input_type,
                    &self.attr("value").unwrap_or_default(),
                ));
            }
            if input_type == "range" && state.value.as_deref() == Some("") {
                state.value = Some(range_default(&self.attr("min"), &self.attr("max")));
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
        if let Some(value) = state.value {
            return value;
        }
        sanitize_input_value(
            &self.input_state_name(),
            &self.attr("value").unwrap_or_default(),
        )
    }

    /// Programmatic value write. Value-mode inputs sanitize into live state
    /// with dirty set; default-mode inputs (`hidden`, buttons, checkable)
    /// write the `value` attribute, which *is* their value.
    /// Returns true when the IDL value actually changed.
    pub(crate) fn set_input_value(&self, value: &str) -> bool {
        let mode = input_value_mode(&self.input_state_name());
        if mode != InputValueMode::Value {
            let before = self.input_value();
            let sanitized = sanitize_input_value(&self.input_state_name(), value);
            self.update_control(|state| {
                state.dirty = false;
                state.user_edited = false;
                state.editing = None;
            });
            let _ = self.set_attr("value", &sanitized);
            return self.input_value() != before;
        }
        let sanitized = sanitize_input_value(&self.input_state_name(), value);
        self.store_input_value(&sanitized, false)
    }

    /// Stores a sanitized value-mode write; fixes range empties to default.
    fn store_input_value(&self, sanitized: &str, by_user: bool) -> bool {
        let mut changed = false;
        self.update_control(|state| {
            if state.value.as_deref() != Some(sanitized) {
                state.value = Some(sanitized.to_string());
                changed = true;
            }
            state.dirty = true;
            state.user_edited = by_user;
            state.editing = None;
        });
        if self.input_state_name() == "range" && self.input_value().is_empty() {
            let fallback = range_default(&self.attr("min"), &self.attr("max"));
            self.update_control(|state| {
                state.value = Some(fallback);
            });
            return true;
        }
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
        if input_type == "number" && !value.is_empty() && !is_valid_float(value) {
            let mut changed = false;
            self.update_control(|state| {
                changed = state.editing.as_deref() != Some(value);
                state.editing = Some(value.to_string());
                state.dirty = true;
                state.user_edited = true;
            });
            return changed;
        }
        let sanitized = sanitize_input_value(&input_type, value);
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
                let live = sanitize_input_value(
                    &self.input_state_name(),
                    &self.attr("value").unwrap_or_default(),
                );
                self.update_control(|state| {
                    state.value = Some(live);
                });
            }
            return;
        }
        if tag == Some("input") && name == "type" {
            self.apply_type_transition();
            return;
        }
        if tag == Some("option") && name == "selected" {
            self.apply_selected_attribute();
        }
    }

    /// Runs the type-change transition against the last observed state.
    fn apply_type_transition(&self) {
        let previous = self.control_state_snapshot().type_seen.unwrap_or_default();
        let current = canonical_input_state(&self.attr("type").unwrap_or_default());
        if previous == current {
            return;
        }
        let previous_mode = input_value_mode(&previous);
        let current_mode = input_value_mode(&current);
        // Value/default/on modes propagate the live value into the attribute;
        // entering value mode from any other mode reloads from the attribute.
        if matches!(
            previous_mode,
            InputValueMode::Value | InputValueMode::Default | InputValueMode::DefaultOn
        ) && matches!(
            current_mode,
            InputValueMode::Default | InputValueMode::DefaultOn
        ) && !self.input_value().is_empty()
        {
            let live = self.input_value();
            self.update_control(|state| {
                state.type_seen = Some(current.clone());
            });
            let _ = self.set_attr("value", &live);
            return;
        }
        if previous_mode != InputValueMode::Value && current_mode == InputValueMode::Value {
            let live = sanitize_input_value(&current, &self.attr("value").unwrap_or_default());
            self.update_control(|state| {
                state.type_seen = Some(current.clone());
                state.value = Some(live);
                state.dirty = false;
                state.user_edited = false;
                state.editing = None;
            });
        } else {
            self.update_control(|state| {
                state.type_seen = Some(current.clone());
            });
        }
        let sanitized = sanitize_input_value(&current, &self.input_value());
        self.update_control(|state| {
            if state.value.is_some() {
                state.value = Some(sanitized.clone());
            }
        });
    }

    /// The `value` content attribute gives the default value; pristine
    /// controls follow it through sanitization.
    pub(crate) fn input_default_value(&self) -> String {
        sanitize_input_value(
            &self.input_state_name(),
            &self.attr("value").unwrap_or_default(),
        )
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
        self.update_control(|state| {
            if state.value.as_deref() != Some(value) {
                state.value = Some(value.to_string());
                changed = true;
            }
            state.dirty = true;
            state.user_edited = by_user;
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
        self.update_control(|state| {
            state.value = Some(text);
        });
    }

    /// Input reset algorithm: flags cleared, value follows the default again.
    pub(crate) fn reset_input(&self) {
        self.update_control(|state| {
            state.value = None;
            state.dirty = false;
            state.user_edited = false;
            state.user_validity = false;
            state.editing = None;
        });
        self.reset_checked();
    }

    /// Textarea reset: raw value returns to child text.
    pub(crate) fn reset_textarea(&self) {
        let text = self.text_content();
        self.update_control(|state| {
            state.value = Some(text);
            state.dirty = false;
            state.user_edited = false;
            state.user_validity = false;
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
        node.update_control(|state| {
            state.default_override = None;
        });
    }

    /// Clears user validity without touching values (select reset helper).
    pub(crate) fn clear_user_validity(&self) {
        self.update_control(|state| {
            state.user_validity = false;
        });
    }

    /// Custom validity message write with newline normalization.
    pub(crate) fn set_custom_message(&self, message: &str) {
        let normalized = message.replace("\r\n", "\n").replace('\r', "\n");
        self.update_control(|state| {
            state.custom_message = normalized;
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
        copy.update_control(|state| {
            state.value = snapshot.value.clone();
            state.dirty = snapshot.dirty;
            state.user_edited = snapshot.user_edited;
            state.editing = snapshot.editing.clone();
        });
    }

    /// Resets every resettable control owned by `form` in tree order.
    pub(crate) fn reset_owned_controls(form: &Node, document: &NodeRef) {
        let form_id = form.id();
        let controls: Vec<NodeRef> = Node::descendants(document)
            .filter(|node| {
                node.form_owner().is_some_and(|owner| owner == form_id) && is_resettable(node)
            })
            .collect();
        for node in controls {
            match node.tag_name() {
                Some("input") => node.reset_input(),
                Some("textarea") => node.reset_textarea(),
                Some("select") => {
                    node.reset_select();
                    node.clear_user_validity();
                }
                Some("output") => Node::reset_output(&node),
                _ => {}
            }
        }
    }
}

fn is_resettable(node: &Node) -> bool {
    matches!(
        node.tag_name(),
        Some("input" | "textarea" | "select" | "output")
    )
}
