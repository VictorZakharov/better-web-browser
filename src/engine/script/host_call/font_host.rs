//! CSS Font Loading's byte validation and renderer-owned font registration.
//! Decode at the host boundary, before a FontFace promise can report success.

use super::super::*;
use serde::Serialize;

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
    loading: bool,
    requested: bool,
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
    if operation == "fontFaceEnvironmentPending" {
        return Ok(Some(JsValue::Boolean(state.font_environment_pending())));
    }
    if operation == "fontFaceEnvironmentObserve" {
        let Some(JsValue::Boolean(observing)) = args.get(1) else {
            return Err(JsNativeError::typ()
                .with_message("font environment observation requires a boolean")
                .into());
        };
        state.font_environment.observing = *observing;
        return Ok(Some(JsValue::Null));
    }
    if operation == "fontFaceCSSFaces" {
        let mut faces = Vec::new();
        for face in state.connected_font_faces() {
            let identity = face.loading_identity();
            let loaded = state
                .loaded_web_fonts
                .iter()
                .any(|font| face.matches_loaded_css_font(font));
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
                loaded,
                loading: state.font_environment.pending_css_fonts.contains(&identity),
                requested: state
                    .font_environment
                    .requested_css_fonts
                    .contains(&identity),
                url: face.url,
                unicode_range: face.unicode_range,
                feature_settings: face.features.css_text(),
            });
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
        state.invalidate_font_layout();
        state.pending_font_actions.push(action);
    }
    Ok(Some(value))
}
