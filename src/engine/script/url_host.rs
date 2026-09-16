//! Shared URL and form-urlencoded primitives for document and worker realms.

use super::binding_helpers::{argument_string, js_string};
use super::{JsNativeError, JsResult, JsValue};
use crate::navigation;

pub(super) fn dispatch(operation: &str, args: &[JsValue]) -> JsResult<Option<JsValue>> {
    let value = match operation {
        "strictResolveUrl" => {
            let input = argument_string(args, 1)?;
            let base = if args.len() > 2 && !matches!(args[2], JsValue::Undefined) {
                Some(argument_string(args, 2)?)
            } else {
                None
            };
            navigation::parse_web_url(&input, base.as_deref())
                .ok_or_else(|| JsNativeError::typ().with_message("Invalid URL"))?
        }
        "parseWebUrl" => {
            let input = argument_string(args, 1)?;
            let parts = navigation::web_url_parts(&input)
                .ok_or_else(|| JsNativeError::typ().with_message("Invalid URL"))?;
            serde_json::to_string(&parts).expect("URL parts serialize")
        }
        "setWebUrlComponent" => {
            let input = argument_string(args, 1)?;
            let component = argument_string(args, 2)?;
            let replacement = argument_string(args, 3)?;
            navigation::set_web_url_component(&input, &component, &replacement)
                .ok_or_else(|| JsNativeError::typ().with_message("Invalid URL component"))?
        }
        "parseFormUrlencoded" => {
            let input = argument_string(args, 1)?;
            // Unlike decodeURIComponent, the URL parser replaces malformed UTF-8
            // and preserves malformed percent escapes rather than throwing.
            let pairs: Vec<_> = url::form_urlencoded::parse(input.as_bytes()).collect();
            serde_json::to_string(&pairs).expect("form pairs serialize")
        }
        "parseFormUrlencodedBytes" => {
            let bytes = args
                .get(1)
                .and_then(JsValue::as_bytes)
                .ok_or_else(|| JsNativeError::typ().with_message("Expected form bytes"))?;
            let pairs: Vec<_> = url::form_urlencoded::parse(bytes).collect();
            serde_json::to_string(&pairs).expect("form pairs serialize")
        }
        _ => return Ok(None),
    };
    Ok(Some(js_string(value)))
}
