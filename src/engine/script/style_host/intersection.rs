//! IntersectionObserver consumes native boxes, not overridable author geometry APIs.
//! https://w3c.github.io/IntersectionObserver/#compute-the-intersection
use super::*;
use crate::engine::css::{Display, Position};
use crate::engine::dom::NodeData;

#[derive(Clone, Copy)]
pub(in crate::engine::script) struct Clip {
    pub rect: RectF,
    pub x: bool,
    pub y: bool,
    pub scroll: bool,
}

pub(in crate::engine::script) struct Geometry {
    pub valid: bool,
    pub target: RectF,
    /// Root-space target for intersection; target stays in its own viewport.
    pub mapped_target: RectF,
    pub root: RectF,
    pub root_scroll: bool,
    pub clips: Vec<Clip>,
}

impl Geometry {
    pub fn value(self) -> JsValue {
        JsValue::Array(vec![
            JsValue::from(self.valid),
            rect_value(self.target),
            rect_value(self.root),
            JsValue::from(self.root_scroll),
            JsValue::Array(
                self.clips
                    .into_iter()
                    .map(|clip| {
                        JsValue::Array(vec![
                            rect_value(clip.rect),
                            JsValue::from(clip.x),
                            JsValue::from(clip.y),
                            JsValue::from(clip.scroll),
                        ])
                    })
                    .collect(),
            ),
            rect_value(self.mapped_target),
        ])
    }
}

pub(super) fn geometry(args: &[JsValue], state: &mut HostState) -> JsValue {
    let target = state.node(argument_id(args, 1));
    let root = state.node(argument_id(args, 2));
    calculate(state, target, root).value()
}

pub(in crate::engine::script) fn calculate(
    state: &mut HostState,
    target: Option<NodeRef>,
    root: Option<NodeRef>,
) -> Geometry {
    state.flush_layout_if_needed();
    let explicit = root.as_ref().filter(|node| node.element().is_some());
    let mut root_scroll = true;
    let mut root_rect = RectF {
        x: 0.0,
        y: 0.0,
        width: state.media_environment.viewport_width,
        height: state.media_environment.viewport_height,
    };
    let mut valid = root.as_ref().is_none_or(|root| state.is_connected(root));
    if root
        .as_ref()
        .is_some_and(|root| matches!(root.data, NodeData::Document))
    {
        valid &= root
            .as_ref()
            .is_some_and(|root| root.id() == state.document.id());
    }
    if let Some(root) = explicit {
        let scroll = state.scroll_boxes.get(&root.id()).copied();
        root_scroll = scroll.is_some_and(|s| s.scroll_x || s.scroll_y);
        let rect = scroll.map(clip_box).or_else(|| bounds(state, root));
        valid &= rect.is_some();
        root_rect = rect
            .map(|rect| viewport_rect(state, root, rect))
            .unwrap_or_default();
    }
    let mut target_rect = RectF::default();
    let mut clips = Vec::new();
    valid &= target.as_ref().is_some_and(|node| state.is_connected(node));
    if let Some(target) = target.filter(|_| valid) {
        let rect = bounds(state, &target);
        valid &= rect.is_some();
        if let Some(rect) = rect {
            target_rect = viewport_rect(state, &target, rect);
            let chain = containing_blocks(state, &target);
            valid &= explicit.is_none_or(|root| chain.iter().any(|node| node.id() == root.id()));
            let (version, mut styles) = state.take_offset_parent_styles();
            for ancestor in chain {
                if explicit.is_some_and(|root| ancestor.id() == root.id()) {
                    break;
                }
                if let Some(scroll) = state.scroll_boxes.get(&ancestor.id()).copied() {
                    let rect = viewport_rect(state, &ancestor, clip_box(scroll));
                    clips.push(Clip {
                        rect,
                        x: scroll.clip_x,
                        y: scroll.clip_y,
                        scroll: scroll.scroll_x || scroll.scroll_y,
                    });
                }
                let shape = styles.computed_style_for_node(&ancestor).and_then(|style| {
                    let border = bounds(state, &ancestor)?;
                    let inset =
                        style
                            .clip_path
                            .inset(border.width, border.height, style.font_size)?;
                    Some(border.inset(inset))
                });
                if let Some(shape) = shape {
                    // A scroll container's basic-shape clip is part of the clip
                    // expanded by scrollMargin; a non-scrolling clip is not.
                    clips.push(Clip {
                        rect: viewport_rect(state, &ancestor, shape),
                        x: true,
                        y: true,
                        scroll: state
                            .scroll_boxes
                            .get(&ancestor.id())
                            .is_some_and(|box_| box_.scroll_x || box_.scroll_y),
                    });
                }
            }
            state.offset_parent_styles = Some((version, styles));
        }
    }
    if !valid {
        root_rect = RectF::default();
    }
    Geometry {
        valid,
        target: target_rect,
        mapped_target: target_rect,
        root: root_rect,
        root_scroll,
        clips,
    }
}

fn clip_box(scroll: crate::engine::layout::ScrollBox) -> RectF {
    // The observer uses the padding edge, including reserved scrollbar gutters,
    // whereas ScrollBox.port excludes gutters for CSSOM client/scroll geometry.
    RectF {
        width: scroll.port.width + if scroll.bar_y { scroll.thickness } else { 0.0 },
        height: scroll.port.height + if scroll.bar_x { scroll.thickness } else { 0.0 },
        ..scroll.port
    }
}

fn bounds(state: &HostState, node: &NodeRef) -> Option<RectF> {
    let Some(rects) = state.layout_fragments.element(node.id()) else {
        return state.layout_geometry.get(&node.id()).copied();
    };
    let mut nonempty = rects.iter().filter(|r| r.width != 0.0 && r.height != 0.0);
    let Some(first) = nonempty.next() else {
        return rects.first().copied();
    };
    Some(nonempty.fold(*first, |a, b| {
        let x = a.x.min(b.x);
        let y = a.y.min(b.y);
        RectF {
            x,
            y,
            width: a.right().max(b.right()) - x,
            height: a.bottom().max(b.bottom()) - y,
        }
    }))
}

pub(in crate::engine::script) fn viewport_rect(
    state: &mut HostState,
    node: &NodeRef,
    mut rect: RectF,
) -> RectF {
    let viewport = state.document.scroll_offset.get();
    let (offset, fixed) = client_rect::scroll_adjustment(state, node, viewport != (0.0, 0.0));
    if !fixed {
        rect.x -= offset.0 + viewport.0;
        rect.y -= offset.1 + viewport.1;
    }
    rect
}

fn rect_value(rect: RectF) -> JsValue {
    JsValue::Array(
        [rect.x, rect.y, rect.width, rect.height]
            .into_iter()
            .map(client_rect::css_pixel)
            .collect(),
    )
}

fn containing_blocks(state: &mut HostState, target: &NodeRef) -> Vec<NodeRef> {
    let (version, mut styles) = state.take_offset_parent_styles();
    let mut chain = Vec::new();
    let mut child = target.clone();
    while let Some(style) = styles.computed_style_for_node(&child) {
        let position = style.position;
        let mut parent = Node::composed_parent(&child);
        let mut found = None;
        while let Some(candidate) = parent {
            parent = Node::composed_parent(&candidate);
            let Some(style) = styles.computed_style_for_node(&candidate) else {
                continue;
            };
            if matches!(style.display, Display::None | Display::Contents) {
                continue;
            }
            let establishes = style.establishes_fixed_position_containing_block();
            let matches = match position {
                Position::Fixed => establishes,
                Position::Absolute => establishes || style.position != Position::Static,
                _ => style.display != Display::Inline,
            };
            if matches {
                found = Some(candidate);
                break;
            }
        }
        let Some(parent) = found else { break };
        child = parent.clone();
        chain.push(parent);
    }
    state.offset_parent_styles = Some((version, styles));
    chain
}
