use super::*;

pub(super) fn root_font_size_for(styles: &HashMap<NodeId, ComputedStyle>, node: &NodeRef) -> f32 {
    let initial = ComputedStyle::initial().font_size;
    let root = std::iter::successors(Some(node.clone()), Node::composed_parent).find(|candidate| {
        matches!(candidate.data, NodeData::Element(_))
            && Node::composed_parent(candidate)
                .is_some_and(|parent| matches!(parent.data, NodeData::Document))
    });
    let Some(root) = root else {
        return initial;
    };
    if root.id() == node.id() {
        return initial;
    }
    styles
        .get(&root.id())
        .map(|style| style.font_size)
        .unwrap_or(initial)
}
