//! HTML activation must work even when no page script requested a JavaScript realm.
use super::*;
use crate::engine::dom::Node;

pub(super) fn activate(target: &NodeRef, document: &NodeRef, outcome: &mut ScriptOutcome) {
    let Some(control) = activation_target(target) else {
        return;
    };
    if crate::engine::css::compile_selector_list(":disabled")
        .unwrap()
        .matches(&control)
    {
        return;
    }
    let version = document.document_mutation_version();
    control.set_checked(control.is_radio() || !control.checked(), true);
    if !control.is_radio() {
        control.set_indeterminate(false);
    }
    if document.document_mutation_version() != version {
        outcome.render_requested = true;
        outcome.invalidation = crate::engine::invalidation::RenderInvalidation {
            roots: vec![document.id()],
            impact: crate::engine::invalidation::MutationKind::State.impact(),
            mutation_count: 0,
            rebuild_style_rules: false,
            removed_nodes: Vec::new(),
        };
    }
}

fn activation_target(target: &NodeRef) -> Option<NodeRef> {
    if target.is_checkable() {
        return Some(target.clone());
    }
    for node in std::iter::successors(Some(target.clone()), |node| node.parent()) {
        if node.tag_name() == Some("label") {
            let control = if let Some(id) = node.attr("for") {
                Node::descendants(&Node::tree_root(&node))
                    .find(|candidate| candidate.attr("id").as_deref() == Some(id.as_str()))
            } else {
                Node::descendants(&node).find(|candidate| {
                    matches!(
                        candidate.tag_name(),
                        Some(
                            "input"
                                | "button"
                                | "select"
                                | "textarea"
                                | "meter"
                                | "output"
                                | "progress"
                        )
                    )
                })
            };
            return control.filter(|control| control.is_checkable());
        }
        if matches!(
            node.tag_name(),
            Some("a" | "button" | "input" | "select" | "textarea")
        ) {
            return None;
        }
    }
    None
}
