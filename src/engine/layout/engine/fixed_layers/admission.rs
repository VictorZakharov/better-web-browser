use super::*;

pub(in crate::engine::layout) fn capture(
    tree: &box_tree::BoxTree<'_>,
    node: &NodeRef,
    style: &ComputedStyle,
    retained: &mut FixedLayers,
) -> bool {
    if style.position != Position::Fixed
        || style.display == Display::None
        || node.is_generated_pseudo()
        || matches!(node.tag_name(), Some("html" | "body"))
        || (style.left == Length::Auto && style.right == Length::Auto)
        || (style.top == Length::Auto && style.bottom == Length::Auto)
        || retained.layers.len() >= MAX_LAYERS
        || !tree.can_scope_inline_style_to_target()
    {
        return false;
    }
    // A retained outer effect/clip has ink bounds computed from its children. Do not
    // splice through it: those bounds may also need rebuilding. Root overflow clips
    // are stable viewport/normal-flow bounds; fixed descendants do not extend them.
    for ancestor in std::iter::successors(Node::composed_parent(node), Node::composed_parent) {
        if ancestor.element().is_none() {
            continue;
        }
        let outer = tree.get(&ancestor);
        if outer.display == Display::None
            || outer.display == Display::Inline
            || outer.position == Position::Fixed
            || outer.position == Position::Relative
            || outer.position == Position::Sticky
            || outer.establishes_fixed_position_containing_block()
            || outer.zoom != 1.0
            || outer.opacity < 1.0
            || !outer.clip_path.is_none()
            || !outer.mask_image.0.is_empty()
            || (outer.overflow_hidden && !matches!(ancestor.tag_name(), Some("html" | "body")))
        {
            return false;
        }
    }
    let mut members = HashSet::new();
    for member in Node::shadow_including_descendants(node) {
        if !insert(&member, &mut members) {
            return false;
        }
    }
    let dom_members = members.clone();
    // Include generated pseudo identities owned by the box tree, not just DOM IDs.
    let mut pending = vec![node.clone()];
    while let Some(member) = pending.pop() {
        if !insert(&member, &mut members) {
            return false;
        }
        pending.extend(tree.children(&member));
    }
    if members.len()
        + retained
            .layers
            .values()
            .map(|v| v.members.len())
            .sum::<usize>()
        > MAX_TOTAL_MEMBERS
    {
        return false;
    }
    retained.layers.insert(
        node.id(),
        FixedLayer {
            dom_members,
            members,
            z_index: style.z_index,
        },
    );
    true
}

fn insert(node: &NodeRef, members: &mut HashSet<NodeId>) -> bool {
    if node.shadow_root().is_some()
        || node.is_fullscreen()
        || node.is_popover_open()
        || matches!(
            node.tag_name(),
            Some("input" | "select" | "textarea" | "button" | "iframe" | "video" | "audio")
        )
    {
        return false;
    }
    members.insert(node.id());
    members.len() <= MAX_MEMBERS
}
