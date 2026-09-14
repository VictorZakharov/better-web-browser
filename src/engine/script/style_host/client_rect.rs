//! CSSOM View fragment snapshots in viewport coordinates.
use super::*;
use crate::engine::script::binding_helpers::argument_id;

pub(super) fn client_rect_host_call(
    operation: &str,
    args: &[JsValue],
    state: &mut HostState,
) -> JsValue {
    state.flush_layout_if_needed();
    let Some(node) = state
        .node(argument_id(args, 1))
        .filter(|n| state.is_connected(n))
    else {
        return JsValue::Array(Vec::new());
    };
    let rects = if operation == "rangeTextRects" {
        state
            .layout_fragments
            .range_text(node.id(), argument_id(args, 2), argument_id(args, 3))
    } else {
        state
            .layout_fragments
            .element(node.id())
            .unwrap_or_else(|| {
                state
                    .layout_geometry
                    .get(&node.id())
                    .copied()
                    .into_iter()
                    .collect()
            })
    };
    // Layout retains document coordinates. Nested scroll offsets and sticky translations
    // are applied here once per node; the JS realm applies its viewport scroll afterwards.
    let own = state
        .sticky_offsets
        .get(&node.id())
        .copied()
        .unwrap_or_default();
    let offset = std::iter::successors(Node::composed_parent(&node), Node::composed_parent).fold(
        (-own.0, -own.1),
        |(x, y), parent| {
            let (dx, dy) = state
                .scroll_boxes
                .get(&parent.id())
                .map_or((0.0, 0.0), |scroll| {
                    scroll.clamp(parent.scroll_offset.get().0, parent.scroll_offset.get().1)
                });
            let (sx, sy) = state
                .sticky_offsets
                .get(&parent.id())
                .copied()
                .unwrap_or_default();
            (x + dx - sx, y + dy - sy)
        },
    );
    let scrolled = args.get(4).is_some_and(JsValue::to_boolean);
    let fixed = (scrolled || offset != (0.0, 0.0)) && state.is_viewport_fixed(&node);
    JsValue::Array(
        rects
            .into_iter()
            .map(|mut rect| {
                if !fixed {
                    rect.x -= offset.0;
                    rect.y -= offset.1;
                }
                JsValue::Array(vec![
                    css_pixel(rect.x),
                    css_pixel(rect.y),
                    css_pixel(rect.width),
                    css_pixel(rect.height),
                    JsValue::from(fixed),
                ])
            })
            .collect(),
    )
}

fn css_pixel(value: f32) -> JsValue {
    // Publish a consistent subpixel grid rather than binary32 accumulation noise.
    // 1/64 CSS px is the layout precision used by Blink's LayoutUnit as well.
    // This applies to every renderer-backed rectangle, not particular pages or tests;
    // script-created DOMRects retain unrestricted double precision.
    JsValue::from((f64::from(value) * 64.0).round() / 64.0)
}
