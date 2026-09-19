//! CSSOM View media-query, viewport-geometry, and document-mode host bindings.

use super::binding_helpers::{argument_id, argument_string, js_string};
use super::*;

pub(super) fn viewport_host_call(
    operation: &str,
    args: &[JsValue],
    state: &mut HostState,
) -> JsResult<Option<JsValue>> {
    let value = match operation {
        "scrollViewport" => {
            state.flush_layout_if_needed();
            let requested = args
                .get(1)
                .and_then(JsValue::as_number)
                .filter(|value| value.is_finite())
                .unwrap_or(0.0);
            let max_scroll = (state.layout_content_height - state.layout_viewport_height).max(0.0);
            let y = requested.clamp(0.0, f64::from(max_scroll)) as f32;
            state.viewport_scroll_y = Some(y);
            state.document.scroll_offset.set((0.0, y));
            JsValue::from(f64::from(y))
        }
        "elementScroll" => {
            state.flush_layout_if_needed();
            let node = state
                .node(argument_id(args, 1))
                .filter(|node| state.is_connected(node));
            let scroll = node
                .as_ref()
                .and_then(|node| state.scroll_boxes.get(&node.id()).copied());
            if let (Some(node), Some(scroll)) = (&node, scroll) {
                let (mut x, mut y) = node.scroll_offset.get();
                if args.len() > 3 {
                    let requested = args[3]
                        .as_number()
                        .filter(|value| value.is_finite())
                        .unwrap_or(0.0)
                        .clamp(f64::from(f32::MIN), f64::from(f32::MAX))
                        as f32;
                    if args[2].as_number() == Some(0.0) {
                        x = requested;
                    } else {
                        y = requested;
                    }
                    let accepted = scroll.clamp(x, y);
                    if node.scroll_offset.replace(accepted) != accepted {
                        state.geometry_scroll_dirty = true;
                        state.timers.request_render();
                    }
                }
                let (x, y) = scroll.clamp(node.scroll_offset.get().0, node.scroll_offset.get().1);
                JsValue::Array(
                    vec![
                        x,
                        y,
                        scroll.content_width,
                        scroll.content_height,
                        scroll.port.width,
                        scroll.port.height,
                    ]
                    .into_iter()
                    .map(|value| JsValue::from(f64::from(value)))
                    .collect(),
                )
            } else {
                JsValue::null()
            }
        }
        "documentScrollHeight" => {
            state.flush_layout_if_needed();
            JsValue::from(f64::from(
                state
                    .layout_content_height
                    .max(state.layout_viewport_height),
            ))
        }
        "mediaMatches" => {
            let query = argument_string(args, 1)?;
            JsValue::from(crate::engine::css::media::media_matches_for_environment(
                &query,
                state.media_environment,
            ))
        }
        "mediaSerialize" => js_string(crate::engine::css::media::serialize_media_query_list(
            &argument_string(args, 1)?,
        )),
        "viewportMetrics" => JsValue::Array(vec![
            JsValue::from(state.media_environment.viewport_width as f64),
            JsValue::from(state.media_environment.viewport_height as f64),
            JsValue::from(state.media_environment.resolution_dppx as f64),
            JsValue::from(state.layout_viewport_width as f64),
            JsValue::from(state.layout_viewport_height as f64),
        ]),
        "documentCompatMode" => js_string(
            (if state.node(argument_id(args, 1)).is_some_and(|node| {
                if node.id() == state.document.id() {
                    state.quirks_mode
                } else {
                    if let Some(metadata) =
                        state.documents.borrow().document_metadata.get(&node.id())
                    {
                        return metadata.quirks;
                    }
                    state
                        .document_streams
                        .parsers
                        .get(&node.id())
                        .is_some_and(|session| {
                            session.parser.dom().quirks_mode.get()
                                != html5ever::tree_builder::QuirksMode::NoQuirks
                        })
                }
            }) {
                "BackCompat"
            } else {
                "CSS1Compat"
            })
            .to_string(),
        ),
        _ => return Ok(None),
    };
    Ok(Some(value))
}
