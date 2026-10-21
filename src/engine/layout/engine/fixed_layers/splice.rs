use super::*;
use std::ops::Range;

pub(super) fn item_range(items: &[DisplayItem], owner: NodeId) -> Option<Range<usize>> {
    let mut start = None;
    let mut result = None;
    for (index, item) in items.iter().enumerate() {
        if let DisplayItem::NodeBoundary { node_id, entering } = item
            && *node_id == owner
        {
            if *entering {
                if start.is_some() || result.is_some() {
                    return None;
                }
                start = Some(index);
            } else {
                result = Some(start.take()?..index + 1);
            }
        }
    }
    if start.is_some() { None } else { result }
}

pub(super) fn node_range(nodes: &[NodeId], members: &HashSet<NodeId>) -> Option<Range<usize>> {
    let start = nodes.iter().position(|id| members.contains(id))?;
    let end = nodes.iter().rposition(|id| members.contains(id))? + 1;
    nodes[start..end]
        .iter()
        .all(|id| members.contains(id))
        .then_some(start..end)
}

pub(super) fn replace(
    output: &mut LayoutOutput,
    items: Range<usize>,
    nodes: Range<usize>,
    owner: NodeId,
    mut layer: LayoutOutput,
) {
    let members = &output.fixed_layers.layers[&owner].members;
    let fragments = std::sync::Arc::make_mut(&mut output.fragments);
    for &id in members {
        fragments.remove(id);
        output.transformed.remove(id);
        output.node_bounds.remove(&id);
        output.resize_boxes.remove(&id);
        output.clip_paths.remove(&id);
        output.hit_excluded.remove(&id);
        output.scroll_boxes.remove(&id);
        output.sticky_offsets.remove(&id);
    }
    fragments.extend_subtree(&layer.fragments, 0);
    output.transformed.extend(layer.transformed);
    output.node_bounds.extend(layer.node_bounds);
    output.resize_boxes.extend(layer.resize_boxes);
    output.clip_paths.extend(layer.clip_paths);
    output.hit_excluded.extend(layer.hit_excluded);
    output.scroll_boxes.extend(layer.scroll_boxes);
    output.sticky_offsets.extend(layer.sticky_offsets);
    output.items.splice(items, layer.items);
    output
        .node_paint_order
        .splice(nodes, layer.node_paint_order);
    if let Some(new) = layer.fixed_layers.layers.remove(&owner) {
        output.fixed_layers.layers.insert(owner, new);
    }
}
