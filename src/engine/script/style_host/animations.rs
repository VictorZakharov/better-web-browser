//! CSS animation discovery and CSSOM syntax validation share native parsers.
use super::*;
use crate::engine::css::stylesheet::keyframes;

pub(super) fn call(
    operation: &str,
    args: &[JsValue],
    state: &mut HostState,
) -> JsResult<Option<JsValue>> {
    Ok(Some(match operation {
        "cssAnimationRevision" => JsValue::from(state.css_animation_revision as f64),
        "cssAnimationSnapshot" => state.css_animation_snapshot(),
        "cssAnimationRendered" => {
            let rendered = state
                .node(argument_id(args, 1))
                .is_some_and(|node| state.css_animation_rendered(&node));
            JsValue::from(rendered)
        }
        "transitionAffectedTargets" => {
            let Some(node) = state.node(argument_id(args, 1)) else {
                return Ok(Some(JsValue::Array(Vec::new())));
            };
            state.transition_affected_targets(
                &node,
                &argument_string(args, 2)?,
                &argument_string(args, 3)?,
            )
        }
        "cssAnimationUnderlying" => {
            let Some(node) = state.node(argument_id(args, 1)) else {
                return Ok(Some(JsValue::Array(Vec::new())));
            };
            let Some(JsValue::Array(properties)) = args.get(2) else {
                return Ok(Some(JsValue::Array(Vec::new())));
            };
            if properties.len() > 64 {
                return Err(JsNativeError::range()
                    .with_message("too many animation properties")
                    .into());
            }
            state.css_animation_underlying(
                &node,
                &properties
                    .iter()
                    .map(JsValue::string_value)
                    .collect::<Vec<_>>(),
            )
        }
        "cssKeyframeName" => {
            keyframes::name(&argument_string(args, 1)?).map_or(JsValue::null(), JsValue::from)
        }
        "cssKeyframeNameText" => JsValue::from(keyframes::name_text(argument_string(args, 1)?)),
        "cssKeyframeOffsets" => {
            keyframes::offsets(&argument_string(args, 1)?).map_or(JsValue::null(), |offsets| {
                JsValue::Array(
                    offsets
                        .into_iter()
                        .map(|v| JsValue::from(f64::from(v)))
                        .collect(),
                )
            })
        }
        "cssKeyframeBlock" => {
            keyframes::block(&argument_string(args, 1)?).map_or(JsValue::null(), |block| {
                JsValue::Array(vec![
                    JsValue::Array(
                        block
                            .offsets
                            .into_iter()
                            .map(|v| JsValue::from(f64::from(v)))
                            .collect(),
                    ),
                    JsValue::Array(
                        block
                            .declarations
                            .into_iter()
                            .map(|(name, value)| {
                                JsValue::Array(vec![JsValue::from(name), JsValue::from(value)])
                            })
                            .collect(),
                    ),
                ])
            })
        }
        _ => return Ok(None),
    }))
}
