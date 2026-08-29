//! Constructed stylesheet snapshots installed on document and shadow roots.

use super::binding_helpers::{argument_id, argument_string, js_string};
use super::*;
use crate::engine::AdoptedStyleSheet;
use crate::limits::{
    MAX_ADOPTED_STYLESHEET_PAYLOAD_BYTES, MAX_ADOPTED_STYLESHEETS, MAX_CSS_SOURCE_BYTES,
};
pub(super) fn cssom_host_call(
    operation: &str,
    args: &[JsValue],
    state: &mut HostState,
) -> JsResult<Option<JsValue>> {
    match operation {
        "stylesheetSource" => {
            let url = argument_string(args, 1)?;
            return Ok(Some(
                state
                    .stylesheet_sources
                    .get(&url)
                    .map_or_else(JsValue::null, |source| js_string(source.clone())),
            ));
        }
        "stylesheetSameOrigin" => {
            let url = argument_string(args, 1)?;
            let same_origin = crate::fetch::Origin::parse(&url)
                .and_then(|origin| {
                    crate::fetch::Origin::parse(&state.document_url)
                        .map(|document_origin| origin.is_same_origin(&document_origin))
                })
                .unwrap_or(false);
            return Ok(Some(JsValue::from(same_origin)));
        }
        "adoptedStyleSheetsSet" => {}
        _ => return Ok(None),
    }
    let Some(root) = state.node(argument_id(args, 1)) else {
        return Err(JsNativeError::typ()
            .with_message("adoptedStyleSheets requires a document or shadow root")
            .into());
    };
    let is_document = state
        .document_roots
        .values()
        .any(|document| document.id() == root.id());
    if !is_document && !matches!(root.data, NodeData::ShadowRoot(_)) {
        return Err(JsNativeError::typ()
            .with_message("adoptedStyleSheets requires a document or shadow root")
            .into());
    }
    let sheets = adopted_sheet_payloads(args.get(2))?;
    if sheets.len() > MAX_ADOPTED_STYLESHEETS {
        return Err(JsNativeError::range()
            .with_message("too many adopted stylesheets on one root")
            .into());
    }
    if sheets
        .iter()
        .any(|stylesheet| stylesheet.source.len() > MAX_CSS_SOURCE_BYTES)
    {
        return Err(JsNativeError::range()
            .with_message("constructed stylesheet exceeds the CSS source limit")
            .into());
    }
    let sheets = sheets
        .into_iter()
        .map(|stylesheet| AdoptedStyleSheet {
            base_url: stylesheet.base_url,
            media: stylesheet.media,
            source: stylesheet.source,
        })
        .collect::<Vec<_>>();
    if root.adopted_stylesheets() != sheets {
        root.set_adopted_stylesheets(sheets);
        state.record_mutation(Some(&root), MutationKind::Stylesheet);
    }
    Ok(Some(JsValue::undefined()))
}

fn adopted_sheet_payloads(value: Option<&JsValue>) -> JsResult<Vec<AdoptedStyleSheet>> {
    let Some(JsValue::Array(records)) = value else {
        return Err(JsNativeError::typ()
            .with_message("adopted stylesheet payload must be an array")
            .into());
    };
    let mut payload_bytes = 0_usize;
    let mut sheets = Vec::with_capacity(records.len());
    for record in records {
        let JsValue::Array(fields) = record else {
            return Err(invalid_adopted_sheet_payload());
        };
        let [
            JsValue::String(base_url),
            JsValue::String(media),
            JsValue::String(source),
        ] = fields.as_slice()
        else {
            return Err(invalid_adopted_sheet_payload());
        };
        payload_bytes = payload_bytes
            .saturating_add(base_url.len())
            .saturating_add(media.len())
            .saturating_add(source.len());
        if payload_bytes > MAX_ADOPTED_STYLESHEET_PAYLOAD_BYTES {
            return Err(JsNativeError::range()
                .with_message("adopted stylesheet payload exceeds the document limit")
                .into());
        }
        sheets.push(AdoptedStyleSheet {
            base_url: base_url.clone(),
            media: media.clone(),
            source: source.clone(),
        });
    }
    Ok(sheets)
}

fn invalid_adopted_sheet_payload() -> JsError {
    JsNativeError::typ()
        .with_message("adopted stylesheet entries must contain base URL, media, and source strings")
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::dom;
    use crate::engine::script::binding_helpers::js_string;

    #[test]
    fn native_snapshot_is_bounded_and_root_owned() {
        let dom = dom::parse("<main></main>");
        let mut state = HostState::new(
            dom.document.clone(),
            "https://example.com/",
            "UTF-8",
            Rc::new(module_loader::WebModuleLoader::new()),
        );
        let document = state.id_for(&dom.document);
        let args = [
            js_string("adoptedStyleSheetsSet".to_string()),
            JsValue::from(document),
            JsValue::Array(vec![JsValue::Array(vec![
                js_string("https://example.com/".to_string()),
                js_string(String::new()),
                js_string("main{color:red}".to_string()),
            ])]),
        ];
        let value = cssom_host_call("adoptedStyleSheetsSet", &args, &mut state)
            .expect("valid stylesheet payload");

        assert!(value.is_some());
        assert_eq!(dom.document.adopted_stylesheets().len(), 1);
        assert!(state.pending_invalidation.snapshot(1).rebuild_style_rules);
    }

    #[test]
    fn native_snapshot_rejects_malformed_and_oversized_entries() {
        let malformed = JsValue::Array(vec![JsValue::Array(vec![js_string(
            "https://example.com/".to_string(),
        )])]);
        let error = adopted_sheet_payloads(Some(&malformed)).expect_err("malformed entry");
        assert!(error.to_string().starts_with("TypeError:"));

        let oversized = JsValue::Array(vec![JsValue::Array(vec![
            js_string("https://example.com/".to_string()),
            js_string(String::new()),
            js_string("x".repeat(MAX_ADOPTED_STYLESHEET_PAYLOAD_BYTES + 1)),
        ])]);
        let error = adopted_sheet_payloads(Some(&oversized)).expect_err("oversized entry");
        assert!(error.to_string().starts_with("RangeError:"));
    }
}
