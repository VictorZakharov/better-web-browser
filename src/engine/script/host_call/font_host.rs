//! CSS Font Loading's byte validation and renderer-owned font registration.
//! Decode at the host boundary, before a FontFace promise can report success.

use super::super::binding_helpers::{argument_id, argument_string};
use super::super::*;
use serde::Serialize;
use std::collections::HashSet;

#[derive(Serialize)]
struct CssFace {
    family: String,
    weight: u16,
    italic: bool,
    url: String,
    loaded: bool,
}

pub(super) fn dispatch(
    operation: &str,
    args: &[JsValue],
    state: &mut HostState,
) -> JsResult<Option<JsValue>> {
    if operation == "fontFaceCSSFaces" {
        let mut faces = Vec::new();
        let mut seen = HashSet::new();
        for source in &state.stylesheet_sources {
            for face in crate::engine::font::discover_font_faces(&source.source, &source.base_url) {
                let key = (
                    face.family.clone(),
                    face.weight,
                    face.italic,
                    face.url.clone(),
                );
                if seen.insert(key) {
                    faces.push(CssFace {
                        family: face.family,
                        weight: face.weight,
                        italic: face.italic,
                        loaded: state.loaded_css_font_urls.contains(&face.url),
                        url: face.url,
                    });
                }
                if faces.len() == 64 {
                    break;
                }
            }
            if faces.len() == 64 {
                break;
            }
        }
        let serialized = serde_json::to_string(&faces)
            .map_err(|error| JsNativeError::typ().with_message(error.to_string()))?;
        return Ok(Some(JsValue::from(JsString::from(serialized))));
    }
    if operation == "fontFaceRemove" {
        state.pending_font_actions.push(ScriptFontAction::Remove {
            id: argument_id(args, 1),
        });
        return Ok(Some(JsValue::undefined()));
    }
    if !matches!(operation, "fontFaceValidate" | "fontFaceInstall") {
        return Ok(None);
    }
    let id = argument_id(args, 1);
    let family = argument_string(args, 2)?;
    let weight = args.get(3).and_then(JsValue::as_number).unwrap_or(400.0);
    let italic = args.get(4).and_then(JsValue::as_boolean).unwrap_or(false);
    let bytes = args.get(5).and_then(JsValue::as_bytes).ok_or_else(|| {
        JsNativeError::typ().with_message("FontFace source must be an ArrayBuffer or view")
    })?;
    if family.is_empty()
        || family.len() > 256
        || !weight.is_finite()
        || !(1.0..=1000.0).contains(&weight)
        || weight.fract() != 0.0
    {
        return Ok(Some(JsValue::Boolean(false)));
    }
    let face = crate::engine::font::WebFontFace {
        family,
        weight: weight as u16,
        weight_min: weight as f32,
        weight_max: weight as f32,
        italic,
        url: format!("fontface:{id}"),
    };
    let Ok(mut font) = crate::engine::font::decode_web_font(&face, bytes) else {
        return Ok(Some(JsValue::Boolean(false)));
    };
    if operation == "fontFaceInstall" {
        font.script_source_id = Some(id);
        state
            .pending_font_actions
            .push(ScriptFontAction::Add { id, font });
    }
    Ok(Some(JsValue::Boolean(true)))
}
