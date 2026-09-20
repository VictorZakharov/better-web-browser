use super::*;
mod select;
pub(super) use select::select_data;

pub(super) fn input_control_data(node: &NodeRef) -> Option<(ControlKind, String)> {
    let tag = node.tag_name()?;
    if tag == "select" {
        return Some((ControlKind::Select, select_data(node).value()));
    }
    if tag == "textarea" {
        return Some((ControlKind::TextArea, node.textarea_api_value()));
    }
    if tag == "button" {
        let kind = match node.attr("type").as_deref() {
            Some(value) if value.eq_ignore_ascii_case("button") => ControlKind::Button,
            Some(value) if value.eq_ignore_ascii_case("reset") => ControlKind::Reset,
            _ => ControlKind::Submit,
        };
        return Some((kind, node.text_content().trim().to_string()));
    }
    if tag != "input" {
        return None;
    }
    let input_type = node
        .attr("type")
        .unwrap_or_else(|| "text".into())
        .to_ascii_lowercase();
    if matches!(
        input_type.as_str(),
        "hidden" | "checkbox" | "radio" | "file"
    ) {
        return None;
    }
    let kind = match input_type.as_str() {
        "password" => ControlKind::Password,
        "search" => ControlKind::Search,
        "submit" => ControlKind::Submit,
        "button" => ControlKind::Button,
        "reset" => ControlKind::Reset,
        _ => ControlKind::Text,
    };
    // Live control state owns the painted value; pristine controls mirror
    // their default through the same accessor scripted getters use.
    Some((kind, node.input_value()))
}

pub(super) fn input_control_label(node: &NodeRef, kind: ControlKind, value: &str) -> String {
    if !matches!(
        kind,
        ControlKind::Submit | ControlKind::Button | ControlKind::Reset
    ) || !value.is_empty()
    {
        return value.to_string();
    }
    // HTML input button labels use value, including an explicitly empty value.
    // Accessible names and tooltips are not substitute visual button labels.
    if node.tag_name() != Some("input") || node.attr("value").is_some() {
        return value.to_string();
    }
    match kind {
        ControlKind::Submit => "Submit".to_string(),
        ControlKind::Reset => "Reset".to_string(),
        _ => String::new(),
    }
}

pub(super) fn default_control_content_height(
    node: &NodeRef,
    kind: &ControlKind,
    style: &ComputedStyle,
) -> f32 {
    match kind {
        ControlKind::Select => style.line_height + 10.0,
        ControlKind::Submit | ControlKind::Button | ControlKind::Reset => 30.0,
        ControlKind::TextArea => {
            node.attr("rows")
                .and_then(|rows| rows.parse::<f32>().ok())
                .unwrap_or(2.0)
                * style.line_height
                + 10.0
        }
        // Text controls derive their auto content box from the line box. CSS padding and borders
        // are added exactly once by normal replaced-element sizing.
        _ => style.line_height,
    }
}

pub(super) fn nearest_form(node: &NodeRef) -> Option<NodeRef> {
    if let Some(form_id) = node.attr("form") {
        let mut root = node.clone();
        while let Some(parent) = root.parent() {
            root = parent;
        }
        if let Some(form) = Node::descendants(&root).find(|candidate| {
            candidate.tag_name() == Some("form")
                && candidate.attr("id").as_deref() == Some(form_id.as_str())
        }) {
            return Some(form);
        }
    }
    let mut ancestor = node.parent();
    while let Some(candidate) = ancestor {
        if candidate.tag_name() == Some("form") {
            return Some(candidate);
        }
        ancestor = candidate.parent();
    }
    None
}

pub(super) fn collect_forms(page: &Page) -> HashMap<NodeId, FormSpec> {
    page.dom
        .elements_named("form")
        .map(|form| {
            let node_id = node_id(&form);
            let action = form
                .attr("action")
                .and_then(|action| resolve_url(&page.source_url, &action))
                .unwrap_or_else(|| page.source_url.clone());
            let method = form
                .attr("method")
                .unwrap_or_else(|| "get".into())
                .to_ascii_lowercase();
            let hidden_fields = Node::descendants(&page.dom.document)
                .filter(|node| node.tag_name() == Some("input"))
                .filter(|node| nearest_form(node).is_some_and(|owner| owner.id() == form.id()))
                .filter(|node| {
                    node.attr("type")
                        .is_some_and(|kind| kind.eq_ignore_ascii_case("hidden"))
                })
                .filter_map(|node| {
                    Some((node.attr("name")?, node.attr("value").unwrap_or_default()))
                })
                .collect();
            (
                node_id,
                FormSpec {
                    node_id,
                    action,
                    method,
                    hidden_fields,
                },
            )
        })
        .collect()
}
