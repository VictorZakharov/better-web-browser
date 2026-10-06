//! CSSOM authorship grammar is resolved natively without exposing the host bridge.
use super::*;
use crate::engine::css::cssom::declarations;

pub(super) fn call(operation: &str, args: &[JsValue]) -> JsResult<Option<JsValue>> {
    Ok(Some(match operation {
        "cssFontLonghands" => JsValue::Array(
            declarations::fonts::LONGHANDS
                .into_iter()
                .map(|name| JsValue::from(name.to_owned()))
                .collect(),
        ),
        "cssFontValue" => {
            let values = (1..=declarations::fonts::LONGHANDS.len())
                .map(|index| argument_string(args, index))
                .collect::<JsResult<Vec<_>>>()?;
            declarations::fonts::serialize(&values).into()
        }
        "cssDeclarationExpansion" => JsValue::Array(
            declarations::expand(&argument_string(args, 1)?, &argument_string(args, 2)?)
                .into_iter()
                .map(|(name, value)| JsValue::Array(vec![name.into(), value.into()]))
                .collect(),
        ),
        "cssFontVariantValue" => {
            declarations::font_variant_value(&argument_string(args, 1)?, &argument_string(args, 2)?)
                .into()
        }
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
