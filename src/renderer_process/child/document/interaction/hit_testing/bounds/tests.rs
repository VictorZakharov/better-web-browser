use super::*;
use crate::engine::RectF;
use crate::engine::dom::{Node, parse};

mod geometry;
mod ownership;
mod work;

fn element(dom: &Dom, id: &str) -> NodeRef {
    Node::shadow_including_descendants(&dom.document)
        .find(|node| node.attr("id").as_deref() == Some(id))
        .unwrap()
}

fn rect(x: f32, y: f32, width: f32, height: f32) -> RectF {
    RectF {
        x,
        y,
        width,
        height,
    }
}

fn layout_for(nodes: &[(&NodeRef, RectF)]) -> LayoutOutput {
    LayoutOutput {
        node_paint_order: nodes.iter().map(|(node, _)| node.id()).collect(),
        node_bounds: nodes
            .iter()
            .map(|(node, rect)| (node.id(), *rect))
            .collect(),
        ..Default::default()
    }
}

fn reference(dom: &Dom, layout: &LayoutOutput, x: f32, y: f32) -> Option<NodeRef> {
    layout.node_paint_order.iter().rev().find_map(|id| {
        let node = dom.find_node(*id)?;
        let rect = layout.visual_rect(&node)?;
        (rect.width > 0.0
            && rect.height > 0.0
            && super::super::super::contains(rect, x, y)
            && !layout.hit_excluded.contains(&node.id())
            && layout.point_in_scroll_clips(&node, x, y))
        .then_some(node)
    })
}

fn assert_equivalent(dom: &Dom, layout: &LayoutOutput, points: &[(f32, f32)]) {
    for &(x, y) in points {
        assert_eq!(
            node_at_point(dom, layout, x, y).map(|node| node.id()),
            reference(dom, layout, x, y).map(|node| node.id()),
            "point ({x}, {y})"
        );
    }
}
