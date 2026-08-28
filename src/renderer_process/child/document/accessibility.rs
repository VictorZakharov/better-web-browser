//! Renderer-owned semantic tree construction and revision-to-revision deltas.

mod semantics;

use self::semantics::{
    accessible_text, actions_for, bounded_text, heading_level, is_disabled, is_read_only, role_for,
};
use super::*;
use crate::engine::dom::{Node, NodeId, NodeRef};
use crate::engine::{ControlSpec, DisplayItem, RectF};
use crate::limits::MAX_ACCESSIBILITY_COORDINATE;
use crate::renderer_protocol::{
    AccessibilityUpdate, DocumentNodeId, PresentedViewport, SemanticNode, SemanticRole,
    SemanticSelection,
};
use std::collections::HashMap;

#[derive(Default)]
pub(super) struct RendererAccessibility {
    previous: HashMap<DocumentNodeId, SemanticNode>,
}

impl RendererAccessibility {
    pub(super) fn update(
        &mut self,
        page: &Page,
        layout: &crate::engine::LayoutOutput,
        viewport: PresentedViewport,
        focused: Option<NodeId>,
        selection: Option<(NodeId, u32, u32)>,
        value_overrides: &HashMap<NodeId, String>,
    ) -> Result<AccessibilityUpdate, String> {
        let root = wire_id(page.dom.document.id())?;
        let bounds = collect_bounds(page, layout, viewport);
        let controls = layout
            .items
            .iter()
            .filter_map(|item| match item {
                DisplayItem::Control(spec) => Some((spec.node_id, spec.as_ref())),
                _ => None,
            })
            .collect::<HashMap<_, _>>();
        let mut next = HashMap::new();
        let roots = build_nodes(
            &page.dom.document,
            page,
            &bounds,
            &controls,
            selection,
            value_overrides,
            &mut next,
        )?;
        if roots != [root] {
            return Err("accessibility document root was not retained".into());
        }
        let focus = focused
            .and_then(|id| wire_id(id).ok())
            .filter(|id| next.contains_key(id))
            .unwrap_or(root);
        let full = self.previous.is_empty();
        let mut nodes = if full {
            next.values().cloned().collect::<Vec<_>>()
        } else {
            next.iter()
                .filter(|(id, node)| self.previous.get(id) != Some(node))
                .map(|(_, node)| node.clone())
                .collect::<Vec<_>>()
        };
        nodes.sort_by_key(|node| node.id.get());
        let mut added = if full {
            Vec::new()
        } else {
            next.keys()
                .filter(|id| !self.previous.contains_key(id))
                .copied()
                .collect::<Vec<_>>()
        };
        added.sort_by_key(|id| id.get());
        let mut removed = if full {
            Vec::new()
        } else {
            self.previous
                .keys()
                .filter(|id| !next.contains_key(id))
                .copied()
                .collect::<Vec<_>>()
        };
        removed.sort_by_key(|id| id.get());
        self.previous = next;
        Ok(AccessibilityUpdate {
            full,
            root,
            focus,
            nodes,
            added,
            removed,
        })
    }
}

fn collect_bounds(
    page: &Page,
    layout: &crate::engine::LayoutOutput,
    viewport: PresentedViewport,
) -> HashMap<NodeId, RectF> {
    let mut bounds = HashMap::new();
    for item in &layout.items {
        match item {
            DisplayItem::Text {
                rect,
                node_id: Some(id),
                ..
            } => union_bound(&mut bounds, *id, *rect),
            DisplayItem::Control(spec) => union_bound(&mut bounds, spec.node_id, spec.rect),
            _ => {}
        }
    }

    // Image display items don't carry an interaction target. Match the renderer-owned image URL
    // and alternative text in DOM order; decorative/mask images have an empty alternative.
    let images = layout
        .items
        .iter()
        .filter_map(|item| match item {
            DisplayItem::Image { rect, url, alt, .. } if !alt.is_empty() => {
                Some((url.as_str(), alt.as_str(), *rect))
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    let mut used = vec![false; images.len()];
    for node in Node::composed_descendants(&page.dom.document)
        .filter(|node| matches!(node.tag_name(), Some("img" | "image" | "svg")))
    {
        let alternative = node
            .attr("alt")
            .or_else(|| node.attr("aria-label"))
            .unwrap_or_default();
        if alternative.is_empty() {
            continue;
        }
        let url = page.image_url(&node);
        if let Some((index, (_, _, rect))) = images.iter().enumerate().find(|(index, item)| {
            !used[*index] && item.1 == alternative && url.as_deref().is_none_or(|url| item.0 == url)
        }) {
            used[index] = true;
            union_bound(&mut bounds, node.id(), *rect);
        }
    }

    let descendants = Node::composed_descendants(&page.dom.document).collect::<Vec<_>>();
    for node in descendants.iter().rev() {
        if let Some(rect) = bounds.get(&node.id()).copied()
            && let Some(parent) = Node::composed_parent(node)
        {
            union_bound(&mut bounds, parent.id(), rect);
        }
    }
    bounds.insert(
        page.dom.document.id(),
        RectF {
            x: 0.0,
            y: 0.0,
            width: viewport.width,
            height: viewport.height,
        },
    );
    bounds
}

fn union_bound(bounds: &mut HashMap<NodeId, RectF>, id: NodeId, next: RectF) {
    let Some(next) = sanitize_bound(next) else {
        return;
    };
    bounds
        .entry(id)
        .and_modify(|current| {
            let right = current.right().max(next.right());
            let bottom = current.bottom().max(next.bottom());
            current.x = current.x.min(next.x);
            current.y = current.y.min(next.y);
            current.width = (right - current.x).clamp(0.0, MAX_ACCESSIBILITY_COORDINATE);
            current.height = (bottom - current.y).clamp(0.0, MAX_ACCESSIBILITY_COORDINATE);
        })
        .or_insert(next);
}

fn sanitize_bound(rect: RectF) -> Option<RectF> {
    if !rect.x.is_finite()
        || !rect.y.is_finite()
        || !rect.width.is_finite()
        || !rect.height.is_finite()
    {
        return None;
    }
    let x = rect
        .x
        .clamp(-MAX_ACCESSIBILITY_COORDINATE, MAX_ACCESSIBILITY_COORDINATE);
    let y = rect
        .y
        .clamp(-MAX_ACCESSIBILITY_COORDINATE, MAX_ACCESSIBILITY_COORDINATE);
    let right = (f64::from(rect.x) + f64::from(rect.width)).clamp(
        f64::from(-MAX_ACCESSIBILITY_COORDINATE),
        f64::from(MAX_ACCESSIBILITY_COORDINATE),
    ) as f32;
    let bottom = (f64::from(rect.y) + f64::from(rect.height)).clamp(
        f64::from(-MAX_ACCESSIBILITY_COORDINATE),
        f64::from(MAX_ACCESSIBILITY_COORDINATE),
    ) as f32;
    Some(RectF {
        x,
        y,
        width: (right - x).clamp(0.0, MAX_ACCESSIBILITY_COORDINATE),
        height: (bottom - y).clamp(0.0, MAX_ACCESSIBILITY_COORDINATE),
    })
}

#[allow(clippy::too_many_arguments)]
fn build_nodes(
    node: &NodeRef,
    page: &Page,
    bounds: &HashMap<NodeId, RectF>,
    controls: &HashMap<NodeId, &ControlSpec>,
    selection: Option<(NodeId, u32, u32)>,
    value_overrides: &HashMap<NodeId, String>,
    output: &mut HashMap<DocumentNodeId, SemanticNode>,
) -> Result<Vec<DocumentNodeId>, String> {
    let role = role_for(node, controls.get(&node.id()).copied());
    let included =
        node.id() == page.dom.document.id() || (role.is_some() && bounds.contains_key(&node.id()));
    let mut children = Vec::new();
    for child in Node::composed_children(node).iter() {
        children.extend(build_nodes(
            child,
            page,
            bounds,
            controls,
            selection,
            value_overrides,
            output,
        )?);
    }
    if !included {
        return Ok(children);
    }

    let id = wire_id(node.id())?;
    let role = role.unwrap_or(SemanticRole::RootWebArea);
    let control = controls.get(&node.id()).copied();
    let (name, value) = accessible_text(node, page, control, role, value_overrides.get(&node.id()));
    let actions = actions_for(role, node, control);
    let selection = selection
        .filter(|(target, _, _)| *target == node.id())
        .map(|(_, start, end)| SemanticSelection { start, end });
    let semantic = SemanticNode {
        id,
        role,
        name,
        value,
        description: bounded_text(
            &node
                .attr("aria-description")
                .or_else(|| node.attr("title"))
                .unwrap_or_default(),
        ),
        bounds: bounds.get(&node.id()).copied().unwrap_or_default(),
        children,
        level: heading_level(node),
        disabled: is_disabled(node),
        read_only: is_read_only(node),
        actions,
        selection,
    };
    if output.insert(id, semantic).is_some() {
        return Err("duplicate accessibility node identity".into());
    }
    Ok(vec![id])
}

fn wire_id(id: NodeId) -> Result<DocumentNodeId, String> {
    DocumentNodeId::new(id.to_wire()).map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_derived_accessibility_bounds_are_sanitized_before_ipc() {
        assert!(
            sanitize_bound(RectF {
                x: f32::INFINITY,
                ..RectF::default()
            })
            .is_none()
        );

        let rect = sanitize_bound(RectF {
            x: -20_000_000.0,
            y: 20_000_000.0,
            width: 40_000_000.0,
            height: -1.0,
        })
        .expect("finite geometry is clipped instead of killing the renderer");
        assert_eq!(rect.x, -MAX_ACCESSIBILITY_COORDINATE);
        assert_eq!(rect.y, MAX_ACCESSIBILITY_COORDINATE);
        assert_eq!(rect.width, MAX_ACCESSIBILITY_COORDINATE);
        assert_eq!(rect.height, 0.0);
    }
}
