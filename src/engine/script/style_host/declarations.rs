//! CSSOM authorship grammar is resolved natively without exposing the host bridge.
use super::*;
use crate::engine::css::cssom::declarations;

pub(super) fn call(operation: &str, args: &[JsValue]) -> JsResult<Option<JsValue>> {
    Ok(Some(match operation {
        "cssDeclarationValue" => {
            declarations::value(&argument_string(args, 1)?, &argument_string(args, 2)?)
                .map_or(JsValue::null(), JsValue::from)
        }
        "cssDeclarationList" => JsValue::Array(
            declarations::list(
                &argument_string(args, 1)?,
                args.get(2).is_some_and(JsValue::to_boolean),
            )
            .into_iter()
            .map(|(name, value, important)| {
                JsValue::Array(vec![
                    JsValue::from(name),
                    JsValue::from(value),
                    JsValue::from(important),
                ])
            })
            .collect(),
        ),
        _ => return Ok(None),
    }))
}
