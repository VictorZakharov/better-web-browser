use super::*;

pub(super) fn input_control_data(node: &NodeRef) -> Option<(ControlKind, String)> {
    let tag = node.tag_name()?;
    if tag == "textarea" {
        return Some((ControlKind::TextArea, node.text_content()));
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
    Some((kind, node.attr("value").unwrap_or_default()))
}

pub(super) fn input_control_label(node: &NodeRef, kind: ControlKind, value: &str) -> String {
    if !matches!(
        kind,
        ControlKind::Submit | ControlKind::Button | ControlKind::Reset
    ) || !value.is_empty()
    {
        return value.to_string();
    }
    let label = node
        .attr("aria-label")
        .or_else(|| node.attr("title"))
        .or_else(|| node.attr("alt"))
        .unwrap_or_default();
    if kind == ControlKind::Submit && label.eq_ignore_ascii_case("search") {
        "Go".to_string()
    } else {
        label
    }
}

pub(super) fn default_control_content_height(
    node: &NodeRef,
    kind: &ControlKind,
    style: &ComputedStyle,
) -> f32 {
    match kind {
        ControlKind::Submit | ControlKind::Button | ControlKind::Reset => 30.0,
        ControlKind::TextArea => {
            node.attr("rows")
                .and_then(|rows| rows.parse::<f32>().ok())
                .unwrap_or(2.0)
                * style.line_height
                + 10.0
        }
        _ => style.line_height + 10.0,
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
