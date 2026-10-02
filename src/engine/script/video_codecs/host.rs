//! Private byte-only native boundary, shared by Window and Worker realms.
use super::*;
use crate::engine::script::{JsNativeError, JsResult, JsValue};
#[cfg(test)]
mod tests;
fn object(entries: Vec<(&str, JsValue)>) -> JsValue {
    JsValue::Object(
        entries
            .into_iter()
            .map(|(key, value)| (key.into(), value))
            .collect(),
    )
}
fn number(args: &[JsValue], index: usize, unsigned: bool) -> JsResult<i64> {
    let value = args
        .get(index)
        .and_then(JsValue::as_number)
        .unwrap_or(f64::NAN);
    if !value.is_finite()
        || value.fract() != 0.0
        || value.abs() > 9_007_199_254_740_991.0
        || unsigned && value < 0.0
    {
        return Err(JsNativeError::range()
            .with_message("video timestamp or duration exceeds exact arithmetic")
            .into());
    }
    Ok(value as i64)
}
fn id(args: &[JsValue]) -> JsResult<u32> {
    let id = number(args, 1, true)?;
    u32::try_from(id).ok().filter(|id| *id != 0).ok_or_else(|| {
        JsNativeError::typ()
            .with_message("invalid video decoder ID")
            .into()
    })
}
fn output(value: Output) -> JsValue {
    object(vec![
        ("bytes", JsValue::Bytes(value.bytes)),
        ("width", JsValue::from(value.width)),
        ("height", JsValue::from(value.height)),
        ("timestamp", JsValue::from(value.timestamp as f64)),
        (
            "duration",
            value
                .duration
                .map_or(JsValue::Null, |value| JsValue::from(value as f64)),
        ),
    ])
}
pub(in crate::engine::script) fn dispatch(
    operation: &str,
    args: &[JsValue],
    codecs: &mut VideoCodecs,
) -> JsResult<Option<JsValue>> {
    let result = match operation {
        "videoCodecSupported" | "videoCodecStart" => {
            let json = args.get(1).map(JsValue::string_value).unwrap_or_default();
            let config = Config::read(&json);
            if operation == "videoCodecSupported" {
                JsValue::from(config.is_ok())
            } else {
                JsValue::from(
                    codecs
                        .start(config.map_err(|error| JsNativeError::typ().with_message(error))?)
                        .map_err(|error| JsNativeError::range().with_message(error))?,
                )
            }
        }
        "videoCodecInput" => {
            let id = id(args)?;
            let bytes = args.get(2).and_then(JsValue::as_bytes).ok_or_else(|| {
                JsNativeError::typ().with_message("video packet bytes are required")
            })?;
            if bytes.is_empty() || bytes.len() > MAX_INPUT {
                return Err(JsNativeError::range()
                    .with_message("video packet requires 1–4 MiB of encoded bytes")
                    .into());
            }
            let timestamp = number(args, 3, false)?;
            let duration = match args.get(4) {
                None | Some(JsValue::Null) => None,
                _ => Some(number(args, 4, true)? as u64),
            };
            let key = matches!(args.get(5), Some(JsValue::Boolean(true)));
            codecs
                .submit(
                    id,
                    Command::Input {
                        bytes: bytes.to_vec(),
                        timestamp,
                        duration,
                        key,
                    },
                )
                .map_err(|error| JsNativeError::range().with_message(error))?;
            JsValue::undefined()
        }
        "videoCodecFlush" => {
            codecs
                .submit(id(args)?, Command::Flush)
                .map_err(|error| JsNativeError::range().with_message(error))?;
            JsValue::undefined()
        }
        "videoCodecPoll" => match codecs.poll(id(args)?) {
            Ok(None) => object(vec![("status", JsValue::from("pending".to_owned()))]),
            Ok(Some(outputs)) => object(vec![
                ("status", JsValue::from("ready".to_owned())),
                (
                    "outputs",
                    JsValue::Array(outputs.into_iter().map(output).collect()),
                ),
            ]),
            Err(error) => object(vec![
                ("status", JsValue::from("error".to_owned())),
                ("message", JsValue::from(error)),
            ]),
        },
        "videoCodecClose" => {
            codecs.close(id(args)?);
            JsValue::undefined()
        }
        _ => return Ok(None),
    };
    Ok(Some(result))
}
