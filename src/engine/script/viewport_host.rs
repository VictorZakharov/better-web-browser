//! CSSOM View media-query, viewport-geometry, and document-mode host bindings.

use super::binding_helpers::{argument_id, argument_string, js_string};
use super::*;

pub(super) fn viewport_host_call(
    operation: &str,
    args: &[JsValue],
    state: &mut HostState,
) -> JsResult<Option<JsValue>> {
    let value =
        match operation {
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
                (if argument_id(args, 1) == state.id_for(&state.document.clone())
                    && state.quirks_mode
                {
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
