//! Select/option selectedness: the select has no value of its own.
//!
//! Each `option` owns selectedness plus dirtiness. The `selected` attribute
//! only affects non-dirty options. Single-select exclusivity applies whenever
//! an option's selectedness becomes true (IDL setters, user picks, insertion
//! of selected options); the selectedness-setting algorithm runs on reset,
//! `selectedIndex` writes that clear selection, option removal, and
//! `multiple`/`size` changes.

use super::{Node, NodeRef};

impl Node {
    /// Options in tree order, descending into optgroups but not into nested
    /// selects, datalists, `hr` separators, or options inside options.
    pub(crate) fn select_options(&self) -> Vec<NodeRef> {
        let mut options = Vec::new();
        let mut stack: Vec<NodeRef> = self.children.borrow().iter().rev().cloned().collect();
        while let Some(node) = stack.pop() {
            let tag = node.tag_name();
            if tag == Some("option") {
                options.push(node);
                continue;
            }
            if matches!(tag, Some("select" | "datalist" | "hr" | "option")) {
                continue;
            }
            if tag == Some("optgroup") && has_optgroup_between(&node, self) {
                continue;
            }
            stack.extend(node.children.borrow().iter().rev().cloned());
        }
        options
    }

    /// Index of the first selected option, or −1.
    pub(crate) fn selected_index(&self) -> i64 {
        self.select_options()
            .iter()
            .position(|option| option.control_state_snapshot().selectedness)
            .map(|index| index as i64)
            .unwrap_or(-1)
    }

    /// Value of the first selected option, or empty when none selected.
    pub(crate) fn select_value(&self) -> String {
        self.select_options()
            .into_iter()
            .find(|option| option.control_state_snapshot().selectedness)
            .map(|option| option.option_value())
            .unwrap_or_default()
    }

    /// Whether the select allows more than one selected option.
    pub(crate) fn select_is_multiple(&self) -> bool {
        self.attr("multiple").is_some()
    }

    /// Display size: `size` attribute, else 4 for multiple, else 1.
    pub(crate) fn select_display_size(&self) -> u64 {
        if let Some(size) = self.attr("size").and_then(|size| parse_non_negative(&size)) {
            return size.max(1);
        }
        if self.select_is_multiple() { 4 } else { 1 }
    }

    /// Runs the selectedness-setting algorithm for this select.
    pub(crate) fn run_selectedness_setting(&self) {
        if self.tag_name() != Some("select") || self.select_is_multiple() {
            return;
        }
        let options = self.select_options();
        let selected: Vec<usize> = options
            .iter()
            .enumerate()
            .filter(|(_, option)| option.control_state_snapshot().selectedness)
            .map(|(index, _)| index)
            .collect();
        if self.select_display_size() == 1 && selected.is_empty() {
            if let Some(first) = options.iter().find(|option| !option_is_disabled(option)) {
                first.update_selectedness(true, false);
            }
            return;
        }
        if selected.len() >= 2 {
            let keep = *selected.last().expect("at least two selected");
            for (index, option) in options.iter().enumerate() {
                if index != keep {
                    option.update_selectedness(false, false);
                }
            }
        }
    }

    /// Sets one option's selectedness (internal; no exclusivity, no events).
    pub(crate) fn update_selectedness(&self, selected: bool, dirty: bool) {
        self.update_control_state(|state| {
            state.selectedness = selected;
            if dirty {
                state.selected_dirty = true;
            }
        });
    }

    /// Enforces single-select exclusivity after an option became selected.
    pub(crate) fn enforce_single_select(&self, except: &Node) {
        let mut ancestor = self.parent();
        while let Some(node) = ancestor {
            if node.tag_name() == Some("select") && !node.select_is_multiple() {
                for option in node.select_options() {
                    if option.id() != except.id() {
                        option.update_selectedness(false, false);
                    }
                }
                return;
            }
            if node.tag_name() == Some("select") {
                return;
            }
            ancestor = node.parent();
        }
    }

    /// `selected` attribute add/remove; honors option dirtiness.
    pub(in crate::engine::dom) fn apply_selected_attribute(&self) {
        if self.tag_name() != Some("option") {
            return;
        }
        let snapshot = self.control_state_snapshot();
        if snapshot.selected_dirty {
            return;
        }
        let present = self.attr("selected").is_some();
        if present == snapshot.selectedness {
            return;
        }
        self.update_selectedness(present, false);
        if present {
            self.enforce_single_select(self);
        }
    }

    /// Insertion hook: a selected option joining a single-select unselects
    /// the others (last in tree order wins across parser insertions).
    pub(in crate::engine::dom) fn control_option_inserted(&self) {
        if self.tag_name() != Some("option") {
            return;
        }
        if self.control_state_snapshot().selectedness {
            self.enforce_single_select(self);
        }
    }

    /// Nearest ancestor select, if any.
    pub(crate) fn nearest_select(&self) -> Option<NodeRef> {
        let mut ancestor = self.parent();
        while let Some(node) = ancestor {
            if node.tag_name() == Some("select") {
                return Some(node);
            }
            ancestor = node.parent();
        }
        None
    }

    /// Native user pick by value: single-select exclusivity, dirtiness, and
    /// user validity. Returns true when the selection actually changed.
    pub(crate) fn user_pick_option(&self, value: &str) -> bool {
        let mut changed = false;
        let mut matched = false;
        for option in self.select_options() {
            let is_match = !matched && option.option_value() == value;
            matched |= is_match;
            if option.control_state_snapshot().selectedness != is_match {
                changed = true;
            }
            option.update_control_state(|state| {
                state.selectedness = is_match;
                state.selected_dirty = true;
            });
        }
        if matched {
            self.update_control_state(|state| {
                state.user_validity = true;
            });
        }
        changed
    }

    /// The select's placeholder label option, if any.
    pub(crate) fn placeholder_option(&self) -> Option<NodeRef> {
        if self.attr("required").is_none()
            || self.select_is_multiple()
            || self.select_display_size() != 1
        {
            return None;
        }
        let options = self.select_options();
        let first = options.first()?;
        if first.option_value().is_empty() && !has_optgroup_between(first, self) {
            return Some(first.clone());
        }
        None
    }

    /// Resets select state from attributes, then runs the setting algorithm.
    pub(crate) fn reset_select(&self) {
        if self.tag_name() != Some("select") {
            return;
        }
        for option in self.select_options() {
            let selected = option.attr("selected").is_some();
            option.update_control_state(|state| {
                state.selectedness = selected;
                state.selected_dirty = false;
            });
        }
        self.run_selectedness_setting();
    }

    /// Option value: `value` attribute, else collapsed text.
    pub(crate) fn option_value(&self) -> String {
        if let Some(value) = self.attr("value") {
            return value;
        }
        collapse_option_text(&self.text_content())
    }

    /// Option label: `label` attribute, else collapsed text.
    pub(crate) fn option_label(&self) -> String {
        if let Some(label) = self.attr("label") {
            return label;
        }
        collapse_option_text(&self.text_content())
    }
}

fn has_optgroup_between(option: &Node, select: &Node) -> bool {
    let mut ancestor = option.parent();
    while let Some(node) = ancestor {
        if node.id() == select.id() {
            return false;
        }
        if node.tag_name() == Some("optgroup") {
            return true;
        }
        ancestor = node.parent();
    }
    false
}

fn option_is_disabled(option: &Node) -> bool {
    if option.attr("disabled").is_some() {
        return true;
    }
    let mut ancestor = option.parent();
    while let Some(node) = ancestor {
        if matches!(
            node.tag_name(),
            Some("select" | "datalist" | "hr" | "option")
        ) {
            return false;
        }
        if node.tag_name() == Some("optgroup") {
            return node.attr("disabled").is_some();
        }
        ancestor = node.parent();
    }
    false
}

fn collapse_option_text(text: &str) -> String {
    let mut out = String::new();
    let mut pending_space = false;
    for chunk in text.split(['\t', '\n', '\x0C', '\r', ' ']) {
        if chunk.is_empty() {
            pending_space = pending_space || !out.is_empty();
            continue;
        }
        if !out.is_empty() || pending_space {
            if !out.is_empty() {
                out.push(' ');
            }
            pending_space = false;
        }
        out.push_str(chunk);
    }
    out
}

fn parse_non_negative(value: &str) -> Option<u64> {
    let trimmed = value.trim_matches(|char| matches!(char, ' ' | '\t' | '\n' | '\x0C' | '\r'));
    if trimmed.is_empty() || trimmed.starts_with(['+', '-']) {
        return None;
    }
    if !trimmed.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    trimmed.parse().ok()
}
