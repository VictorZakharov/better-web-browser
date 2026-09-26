//! Per-occurrence CSSOM updates share bytes for loading, never mutable rule identities.
use super::*;
use crate::engine::css::imports::{self, SheetOverride};

pub(super) fn call(
    operation: &str,
    args: &[JsValue],
    state: &mut HostState,
) -> JsResult<Option<JsValue>> {
    match operation {
        "stylesheetImport" => {
            let text = argument_string(args, 1)?;
            let value = imports::parse(&text)
                .into_iter()
                .next()
                .map_or_else(JsValue::null, |i| {
                    JsValue::Array(vec![
                        js_string(i.href),
                        js_string(i.media),
                        i.supports.map_or_else(JsValue::null, js_string),
                        i.layer.map_or_else(JsValue::null, js_string),
                        i.scope.map_or_else(JsValue::null, js_string),
                    ])
                });
            return Ok(Some(value));
        }
        "stylesheetBase" => {
            let url = argument_string(args, 1)?;
            return Ok(Some(js_string(
                state
                    .stylesheet_sources
                    .iter()
                    .rev()
                    .find(|s| s.url() == url.split('#').next().unwrap_or(&url))
                    .map_or(url.clone(), |s| s.base_url.clone()),
            )));
        }
        "stylesheetDisabled"
        | "stylesheetDisable"
        | "stylesheetOverrides"
        | "stylesheetGeneration" => {}
        _ => return Ok(None),
    }
    let Some(node) = state.node(argument_id(args, 1)) else {
        return Ok(Some(JsValue::null()));
    };
    match operation {
        "stylesheetGeneration" => return Ok(Some(JsValue::from(node.sheet_generation()))),
        "stylesheetDisabled" => return Ok(Some(JsValue::from(Node::sheet_disabled(&node)))),
        "stylesheetDisable" => {
            node.set_sheet_disabled(matches!(args.get(2), Some(JsValue::Boolean(true))))
        }
        _ => {
            let Some(JsValue::Array(records)) = args.get(2) else {
                return Err(invalid_adopted_sheet_payload());
            };
            if records.len() > 257 {
                return Err(invalid_adopted_sheet_payload());
            }
            let mut bytes = 0usize;
            let mut overrides = Vec::new();
            for record in records {
                let JsValue::Array(fields) = record else {
                    return Err(invalid_adopted_sheet_payload());
                };
                let [
                    JsValue::String(path),
                    JsValue::String(source),
                    JsValue::String(media),
                    JsValue::Boolean(disabled),
                ] = fields.as_slice()
                else {
                    return Err(invalid_adopted_sheet_payload());
                };
                bytes = bytes
                    .saturating_add(source.len())
                    .saturating_add(media.len())
                    .saturating_add(path.len());
                if bytes > MAX_ADOPTED_STYLESHEET_PAYLOAD_BYTES
                    || source.len() > MAX_CSS_SOURCE_BYTES
                {
                    return Err(invalid_adopted_sheet_payload());
                }
                let path = if path.is_empty() {
                    Vec::new()
                } else {
                    path.split('.')
                        .map(str::parse::<usize>)
                        .collect::<Result<Vec<_>, _>>()
                        .map_err(|_| invalid_adopted_sheet_payload())?
                };
                if path.len() >= crate::limits::MAX_CSS_NESTING_DEPTH {
                    return Err(invalid_adopted_sheet_payload());
                }
                overrides.push(SheetOverride {
                    path,
                    source: source.clone(),
                    media: media.clone(),
                    disabled: *disabled,
                });
            }
            node.set_sheet_overrides(overrides);
        }
    }
    state.record_mutation(Some(&node), MutationKind::Stylesheet);
    Ok(Some(JsValue::undefined()))
}
