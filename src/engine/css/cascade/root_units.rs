use super::*;

pub(super) fn root_font_size_for(
    styles: &HashMap<NodeId, ComputedStyle>,
    node: &NodeRef,
    parent_style: Option<&ComputedStyle>,
) -> f32 {
    let initial = || ComputedStyle::initial().font_size;
    let Some(parent) = Node::composed_parent(node) else {
        return initial();
    };
    if matches!(parent.data, NodeData::Document) {
        return initial();
    }
    let parent_is_root = matches!(parent.data, NodeData::Element(_))
        && Node::composed_parent(&parent)
            .is_some_and(|grandparent| matches!(grandparent.data, NodeData::Document));
    if parent_is_root {
        return parent_style
            .or_else(|| styles.get(&parent.id()))
            .map_or_else(initial, |style| style.font_size);
    }
    // Below the root's children every element sees the root through its parent, whose style is
    // computed first. This avoids walking to the root for each of a large document's elements.
    if let Some(parent_style) = parent_style {
        return parent_style.root_font_size;
    }
    let root = std::iter::successors(Some(node.clone()), Node::composed_parent).find(|candidate| {
        matches!(candidate.data, NodeData::Element(_))
            && Node::composed_parent(candidate)
                .is_some_and(|parent| matches!(parent.data, NodeData::Document))
    });
    root.and_then(|root| styles.get(&root.id()))
        .map_or_else(initial, |style| style.font_size)
}
