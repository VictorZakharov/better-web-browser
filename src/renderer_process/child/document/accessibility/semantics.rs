//! DOM-to-semantic role, name, state, and action mapping.

use crate::engine::dom::{NodeData, NodeRef};
use crate::engine::{ControlKind, ControlSpec, Page};
use crate::limits::MAX_ACCESSIBILITY_NODE_TEXT_BYTES;
use crate::renderer_protocol::{SemanticActions, SemanticRole};

pub(super) fn role_for(node: &NodeRef, control: Option<&ControlSpec>) -> Option<SemanticRole> {
    if matches!(node.data, NodeData::Document) {
        return Some(SemanticRole::RootWebArea);
    }
    if matches!(node.data, NodeData::Text(_)) {
        return (!normalized_text(&node.text_content()).is_empty())
            .then_some(SemanticRole::TextRun);
    }
    if let Some(role) = node.attr("role") {
        if matches!(
            role.trim().to_ascii_lowercase().as_str(),
            "none" | "presentation"
        ) {
            return None;
        }
        if let Some(role) = aria_role(&role) {
            return Some(role);
        }
    }
    if let Some(control) = control {
        return Some(match control.kind {
            ControlKind::Text => SemanticRole::TextInput,
            ControlKind::TextArea => SemanticRole::MultilineTextInput,
            ControlKind::Password => SemanticRole::PasswordInput,
            ControlKind::Search => SemanticRole::SearchInput,
            ControlKind::Select => SemanticRole::ComboBox,
            ControlKind::Submit | ControlKind::Button | ControlKind::Reset => SemanticRole::Button,
        });
    }
    match node.tag_name()? {
        "p" => Some(SemanticRole::Paragraph),
        "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => Some(SemanticRole::Heading),
        "a" if node.attr("href").is_some() => Some(SemanticRole::Link),
        "ul" | "ol" => Some(SemanticRole::List),
        "li" => Some(SemanticRole::ListItem),
        "table" => Some(SemanticRole::Table),
        "thead" | "tbody" | "tfoot" => Some(SemanticRole::RowGroup),
        "tr" => Some(SemanticRole::Row),
        "td" => Some(SemanticRole::Cell),
        "th" if node.attr("scope").as_deref() == Some("row") => Some(SemanticRole::RowHeader),
        "th" => Some(SemanticRole::ColumnHeader),
        "img" | "image" | "svg" => Some(SemanticRole::Image),
        "form" => Some(SemanticRole::Form),
        "main" => Some(SemanticRole::Main),
        "nav" => Some(SemanticRole::Navigation),
        "header" => Some(SemanticRole::Header),
        "footer" => Some(SemanticRole::Footer),
        "article" => Some(SemanticRole::Article),
        "section" if node.attr("aria-label").is_some() => Some(SemanticRole::Section),
        _ => None,
    }
}

fn aria_role(role: &str) -> Option<SemanticRole> {
    match role.trim().to_ascii_lowercase().as_str() {
        "heading" => Some(SemanticRole::Heading),
        "link" => Some(SemanticRole::Link),
        "button" => Some(SemanticRole::Button),
        "list" => Some(SemanticRole::List),
        "listitem" => Some(SemanticRole::ListItem),
        "table" | "grid" => Some(SemanticRole::Table),
        "row" => Some(SemanticRole::Row),
        "cell" | "gridcell" => Some(SemanticRole::Cell),
        "rowheader" => Some(SemanticRole::RowHeader),
        "columnheader" => Some(SemanticRole::ColumnHeader),
        "img" => Some(SemanticRole::Image),
        "form" => Some(SemanticRole::Form),
        "main" => Some(SemanticRole::Main),
        "navigation" => Some(SemanticRole::Navigation),
        "article" => Some(SemanticRole::Article),
        "region" => Some(SemanticRole::Section),
        "none" | "presentation" => None,
        _ => None,
    }
}

pub(super) fn accessible_text(
    node: &NodeRef,
    page: &Page,
    control: Option<&ControlSpec>,
    role: SemanticRole,
    value_override: Option<&String>,
) -> (String, String) {
    if role == SemanticRole::RootWebArea {
        return (bounded_text(&page.title), String::new());
    }
    if let Some(control) = control {
        let name = node
            .attr("aria-label")
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| {
                [&control.label, &control.name, &control.placeholder]
                    .into_iter()
                    .find(|value| !value.trim().is_empty())
                    .cloned()
                    .unwrap_or_default()
            });
        let value = match role {
            SemanticRole::PasswordInput => String::new(),
            SemanticRole::TextInput
            | SemanticRole::SearchInput
            | SemanticRole::MultilineTextInput
            | SemanticRole::ComboBox => value_override
                .cloned()
                .unwrap_or_else(|| control.value.clone()),
            _ => control.value.clone(),
        };
        return (bounded_text(&name), bounded_text(&value));
    }
    let name = node
        .attr("aria-label")
        .or_else(|| {
            (role == SemanticRole::Image)
                .then(|| node.attr("alt"))
                .flatten()
        })
        .unwrap_or_else(|| normalized_text(&node.text_content()));
    let value = if role == SemanticRole::TextRun {
        normalized_text(&node.text_content())
    } else {
        String::new()
    };
    (bounded_text(&name), bounded_text(&value))
}

pub(super) fn actions_for(
    role: SemanticRole,
    node: &NodeRef,
    control: Option<&ControlSpec>,
) -> SemanticActions {
    let disabled = is_disabled(node);
    let editable = matches!(
        role,
        SemanticRole::TextInput
            | SemanticRole::MultilineTextInput
            | SemanticRole::PasswordInput
            | SemanticRole::SearchInput
            | SemanticRole::ComboBox
    );
    SemanticActions {
        focus: !disabled && (control.is_some() || role == SemanticRole::Link),
        invoke: !disabled && matches!(role, SemanticRole::Link | SemanticRole::Button),
        set_value: !disabled && editable && !is_read_only(node),
    }
}

pub(super) fn is_disabled(node: &NodeRef) -> bool {
    node.attr("disabled").is_some()
        || node
            .attr("aria-disabled")
            .is_some_and(|value| value.eq_ignore_ascii_case("true"))
}

pub(super) fn is_read_only(node: &NodeRef) -> bool {
    node.attr("readonly").is_some()
        || node
            .attr("aria-readonly")
            .is_some_and(|value| value.eq_ignore_ascii_case("true"))
}

pub(super) fn heading_level(node: &NodeRef) -> Option<u32> {
    node.attr("aria-level")
        .and_then(|value| value.parse::<u32>().ok())
        .filter(|level| *level > 0)
        .or_else(|| {
            node.tag_name()
                .and_then(|tag| tag.strip_prefix('h'))
                .and_then(|level| level.parse::<u32>().ok())
                .filter(|level| (1..=6).contains(level))
        })
}

fn normalized_text(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub(super) fn bounded_text(text: &str) -> String {
    crate::limits::bounded_utf8_prefix(text, MAX_ACCESSIBILITY_NODE_TEXT_BYTES)
        .0
        .to_string()
}
