//! CSS Font Loading's byte validation and renderer-owned font registration.
//! Decode at the host boundary, before a FontFace promise can report success.

use super::super::*;
use serde::Serialize;
use std::collections::HashSet;

mod matching;
pub(in crate::engine::script) mod resources;

#[derive(Serialize)]
struct CssFace {
    family: String,
    weight: u16,
    italic: bool,
    url: String,
    source: String,
    loaded: bool,
    #[serde(rename = "unicodeRange")]
    unicode_range: String,
    #[serde(rename = "featureSettings")]
    feature_settings: String,
}

pub(super) fn dispatch(
    operation: &str,
    args: &[JsValue],
    state: &mut HostState,
) -> JsResult<Option<JsValue>> {
    if operation == "fontFaceCSSFaces" {
        let mut faces = Vec::new();
        let mut seen = HashSet::new();
        for face in state.document_font_faces() {
            let key = (
                face.family.clone(),
                face.weight,
                face.italic,
                face.url.clone(),
                face.unicode_range.clone(),
                face.features.clone(),
                face.fallback_urls.clone(),
            );
            if seen.insert(key) {
                let mut source = String::new();
                for url in std::iter::once(&face.url).chain(&face.fallback_urls) {
                    if !source.is_empty() {
                        source.push_str(", ");
                    }
                    source.push_str("url(");
                    cssparser::serialize_string(url, &mut source)
                        .map_err(|error| JsNativeError::typ().with_message(error.to_string()))?;
                    source.push(')');
                }
                faces.push(CssFace {
                    source,
                    family: face.family,
                    weight: face.weight,
                    italic: face.italic,
                    loaded: state.loaded_css_font_urls.contains(&face.url),
                    url: face.url,
                    unicode_range: face.unicode_range,
                    feature_settings: face.features.css_text(),
                });
            }
            if faces.len() == 64 {
                break;
            }
        }
        let serialized = serde_json::to_string(&faces)
            .map_err(|error| JsNativeError::typ().with_message(error.to_string()))?;
        return Ok(Some(JsValue::from(JsString::from(serialized))));
    }
    let Some((value, action)) = resources::dispatch(operation, args, &mut state.loaded_web_fonts)?
    else {
        return Ok(None);
    };
    if let Some(action) = action {
        state.pending_font_actions.push(action);
    }
    Ok(Some(value))
}
