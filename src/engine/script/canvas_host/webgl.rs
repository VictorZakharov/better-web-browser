//! Realm-owned native WebGL bridge. Unsupported platforms decline context creation.
#[cfg(windows)]
use super::super::binding_helpers::{argument_id, argument_string};
use super::super::*;

#[cfg(windows)]
pub(crate) use crate::engine::webgl::Contexts;
#[cfg(not(windows))]
#[derive(Default)]
pub(crate) struct Contexts;
#[cfg(not(windows))]
impl Contexts {
    pub(crate) fn clear(&mut self) {}
    pub(crate) fn complete_task(&mut self) {}
}

pub(crate) fn dispatch(
    operation: &str,
    args: &[JsValue],
    contexts: &mut Contexts,
) -> JsResult<Option<JsValue>> {
    if !matches!(
        operation,
        "webglCreate" | "webglDestroy" | "webglCommand" | "webglReadPixels" | "webglSnapshot"
    ) {
        return Ok(None);
    }
    #[cfg(not(windows))]
    {
        let _ = (args, contexts);
        Ok(Some(JsValue::Null))
    }
    #[cfg(windows)]
    {
        let id = argument_id(args, 1);
        let value = match operation {
            "webglCreate" => {
                let height = argument_id(args, 2);
                let options = argument_string(args, 3)?;
                contexts
                    .create(id, height, &options)
                    .map_or(JsValue::Null, JsValue::from)
            }
            "webglDestroy" => {
                contexts.remove(id);
                JsValue::Undefined
            }
            "webglCommand" => {
                let command = argument_string(args, 2)?;
                let value = contexts.execute(id, &command, args.get(3).and_then(JsValue::as_bytes));
                JsValue::String(value.to_string())
            }
            "webglReadPixels" => {
                use crate::engine::webgl::PixelReply;
                let command = argument_string(args, 2)?;
                match contexts.read_pixels(id, &command, args.get(3).and_then(JsValue::as_bytes)) {
                    PixelReply::Bytes(bytes) => JsValue::Bytes(bytes),
                    PixelReply::Error => JsValue::Null,
                    PixelReply::Lost => JsValue::String(r#"{"lost":true}"#.into()),
                }
            }
            "webglSnapshot" => {
                contexts
                    .snapshot(id)
                    .map_or(JsValue::Null, |(width, height, pixels)| {
                        JsValue::Array(vec![
                            JsValue::from(width),
                            JsValue::from(height),
                            JsValue::Bytes(pixels),
                        ])
                    })
            }
            _ => unreachable!(),
        };
        Ok(Some(value))
    }
}
