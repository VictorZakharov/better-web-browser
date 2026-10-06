//! Shared FontFace byte admission. Each document/worker owns a separate registry;
//! only document admission additionally schedules a page-layout notification.

use super::super::super::binding_helpers::{argument_id, argument_string};
use super::super::super::*;

pub(in crate::engine::script) fn dispatch(
    operation: &str,
    args: &[JsValue],
    fonts: &mut Vec<crate::engine::WebFont>,
) -> JsResult<Option<(JsValue, Option<ScriptFontAction>)>> {
    if let Some(value) = super::matching::dispatch(operation, args)? {
        return Ok(Some((value, None)));
    }
    if operation == "fontFaceRemove" {
        let id = argument_id(args, 1);
        fonts.retain(|font| font.script_source_id != Some(id));
        return Ok(Some((
            JsValue::undefined(),
            Some(ScriptFontAction::Remove { id }),
        )));
    }
    if !matches!(operation, "fontFaceValidate" | "fontFaceInstall") {
        return Ok(None);
    }
    let id = argument_id(args, 1);
    let family = argument_string(args, 2)?;
    let Some(family) = super::matching::family(&family) else {
        return Ok(Some((JsValue::Boolean(false), None)));
    };
    let weight = args.get(3).and_then(JsValue::as_number).unwrap_or(400.0);
    let italic = args.get(4).and_then(JsValue::as_boolean).unwrap_or(false);
    let ranges = args
        .get(6)
        .map(JsValue::string_value)
        .unwrap_or_else(|| "U+0-10FFFF".into());
    let Some(ranges) = crate::engine::font::unicode_ranges::UnicodeRanges::parse(&ranges) else {
        return Ok(Some((JsValue::Boolean(false), None)));
    };
    let features = args
        .get(7)
        .map(JsValue::string_value)
        .unwrap_or_else(|| "normal".into());
    let Some(features) = crate::engine::css::FontFeatures::parse(&features) else {
        return Ok(Some((JsValue::Boolean(false), None)));
    };
    let bytes = args.get(5).and_then(JsValue::as_bytes).ok_or_else(|| {
        JsNativeError::typ().with_message("FontFace source must be an ArrayBuffer or view")
    })?;
    let rejected = || Some((JsValue::Boolean(false), None));
    if family.is_empty()
        || family.len() > 256
        || !weight.is_finite()
        || !(1.0..=1000.0).contains(&weight)
        || weight.fract() != 0.0
    {
        return Ok(rejected());
    }
    let face = crate::engine::font::WebFontFace {
        family,
        weight: weight as u16,
        weight_min: weight as f32,
        weight_max: weight as f32,
        italic,
        url: format!("fontface:{id}"),
        fallback_urls: Vec::new(),
        unicode_range: ranges.serialize(),
        features,
    };
    let Ok(mut font) = crate::engine::font::decode_web_font(&face, bytes) else {
        return Ok(rejected());
    };
    font.unicode_ranges = ranges;
    let action = if operation == "fontFaceInstall" {
        font.script_source_id = Some(id);
        if let Some(existing) = fonts
            .iter_mut()
            .find(|font| font.script_source_id == Some(id))
        {
            *existing = font.clone();
        } else if fonts.len() < crate::limits::MAX_WEB_FONTS {
            fonts.push(font.clone());
        } else {
            return Ok(rejected());
        }
        Some(ScriptFontAction::Add { id, font })
    } else {
        None
    };
    Ok(Some((JsValue::Boolean(true), action)))
}
