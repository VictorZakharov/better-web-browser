//! Byte-only private protocol shared by Window and dedicated workers.
use super::{AudioCodecs, Command, Output, config::Config};
use crate::engine::script::{JsNativeError, JsResult, JsValue};

#[cfg(test)]
mod tests;

fn id(args: &[JsValue]) -> JsResult<u32> {
    let value = args.get(1).and_then(JsValue::as_number).unwrap_or(f64::NAN);
    if !value.is_finite() || value.fract() != 0.0 || !(1.0..=u32::MAX as f64).contains(&value) {
        return Err(JsNativeError::typ()
            .with_message("invalid audio codec session ID")
            .into());
    }
    Ok(value as u32)
}

fn object(entries: Vec<(&str, JsValue)>) -> JsValue {
    JsValue::Object(
        entries
            .into_iter()
            .map(|(key, value)| (key.into(), value))
            .collect(),
    )
}

fn output(value: Output) -> JsValue {
    object(vec![
        ("bytes", JsValue::Bytes(value.bytes)),
        ("format", JsValue::from(value.format.to_owned())),
        ("timestamp", JsValue::from(value.timestamp as f64)),
        ("duration", JsValue::from(value.duration as f64)),
        ("frames", JsValue::from(value.frames)),
        (
            "description",
            value.description.map_or(JsValue::Null, JsValue::Bytes),
        ),
    ])
}

pub(in crate::engine::script) fn dispatch(
    operation: &str,
    args: &[JsValue],
    codecs: &mut AudioCodecs,
) -> JsResult<Option<JsValue>> {
    let value = match operation {
        "audioCodecSupported" | "audioCodecStart" => {
            let encode = matches!(args.get(1), Some(JsValue::Boolean(true)));
            let json = args.get(2).map(JsValue::string_value).unwrap_or_default();
            let config = Config::read(&json, encode);
            if operation == "audioCodecSupported" {
                JsValue::from(config.is_ok())
            } else {
                let config = config.map_err(|e| JsNativeError::typ().with_message(e))?;
                JsValue::from(
                    codecs
                        .start(config, encode)
                        .map_err(|e| JsNativeError::range().with_message(e))?,
                )
            }
        }
        "audioCodecInput" => {
            let id = id(args)?;
            let bytes = args.get(2).and_then(JsValue::as_bytes).ok_or_else(|| {
                JsNativeError::typ().with_message("audio codec input bytes are required")
            })?;
            if bytes.len() > super::MAX_INPUT_BYTES {
                return Err(JsNativeError::range()
                    .with_message("audio codec command exceeds its byte limit")
                    .into());
            }
            let timestamp = args.get(3).and_then(JsValue::as_number).unwrap_or(f64::NAN);
            // Internal bridge uses exact safe timestamps. Web IDL accepts wider
            // values, but codec arithmetic must fail closed rather than saturate.
            if !timestamp.is_finite()
                || timestamp.fract() != 0.0
                || timestamp.abs() > 9_007_199_254_740_991.0
            {
                return Err(JsNativeError::range()
                    .with_message("audio codec timestamp exceeds exact arithmetic")
                    .into());
            }
            codecs
                .submit(
                    id,
                    Command::Input {
                        bytes: bytes.to_vec(),
                        timestamp: timestamp as i64,
                    },
                )
                .map_err(|e| JsNativeError::range().with_message(e))?;
            JsValue::undefined()
        }
        "audioCodecFlush" => {
            codecs
                .submit(id(args)?, Command::Flush)
                .map_err(|e| JsNativeError::range().with_message(e))?;
            JsValue::undefined()
        }
        "audioCodecPoll" => match codecs.poll(id(args)?) {
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
        "audioCodecClose" => {
            codecs.close(id(args)?);
            JsValue::undefined()
        }
        _ => return Ok(None),
    };
    Ok(Some(value))
}
