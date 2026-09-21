//! Form-level control integration: reset and static invalid enumeration.
//!
//! Reset restores each owned control in tree order (including external
//! `form=` controls and detached subtrees); static validation lists failing
//! owned controls for submission gating and interactive reporting.
//! Per-control reset steps live beside their state.

use super::{Node, NodeRef};

/// Resets every resettable control owned by `form` in tree order.
pub(crate) fn reset_owned_controls(form: &Node, document: &NodeRef) {
    let form_id = form.id();
    let mut controls: Vec<NodeRef> = Node::descendants(document)
        .filter(|node| {
            node.form_owner().is_some_and(|owner| owner == form_id) && is_resettable(node)
        })
        .collect();
    // Document scans cannot see detached subtrees, so append owned form
    // descendants missed above (skipping descendants associated elsewhere
    // via `form=` and anything already collected).
    collect_owned_descendants(form, form_id, &mut controls);
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

fn collect_owned_descendants(form: &Node, form_id: super::NodeId, controls: &mut Vec<NodeRef>) {
    for child in form.children.borrow().iter() {
        if child.form_owner().is_some_and(|owner| owner == form_id)
            && is_resettable(child)
            && !controls.iter().any(|seen| seen.id() == child.id())
        {
            controls.push(child.clone());
        }
        collect_owned_descendants(child, form_id, controls);
    }
}

/// Static validation over a form's owned submittable controls in tree
/// order. Fires no events itself; callers dispatch `invalid` where
/// listeners exist.
pub(crate) fn static_invalid_controls(
    form: &Node,
    document: &NodeRef,
    patterns: &super::control_validity::PatternSource,
) -> Vec<NodeRef> {
    use super::control_validity::{validity_of, will_validate};
    let form_id = form.id();
    Node::descendants(document)
        .filter(|node| {
            node.form_owner().is_some_and(|owner| owner == form_id)
                && matches!(
                    node.tag_name(),
                    Some("input" | "button" | "select" | "textarea")
                )
                && will_validate(node)
                && !validity_of(node, patterns).valid()
        })
        .collect()
}

fn is_resettable(node: &Node) -> bool {
    matches!(
        node.tag_name(),
        Some("input" | "textarea" | "select" | "output")
    )
}
