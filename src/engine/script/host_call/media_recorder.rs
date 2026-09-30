//! Private bridge between live PCM capture and bounded per-document encoders.

use super::super::*;
use super::argument_string;

fn positive_integer(args: &[JsValue], index: usize) -> JsResult<u32> {
    integer(args, index, 1.0)
}

fn integer(args: &[JsValue], index: usize, minimum: f64) -> JsResult<u32> {
    let value = args
        .get(index)
        .and_then(JsValue::as_number)
        .filter(|value| {
            value.is_finite()
                && value.fract() == 0.0
                && *value >= minimum
                && *value <= u32::MAX as f64
        })
        .ok_or_else(|| JsNativeError::range().with_message("Invalid audio recorder argument"))?;
    Ok(value as u32)
}

pub(super) fn dispatch(
    operation: &str,
    args: &[JsValue],
    state: &mut HostState,
) -> JsResult<Option<JsValue>> {
    match operation {
        "mediaRecorderType" => Ok(Some(JsValue::from(
            crate::media_type::recording_kind(&argument_string(args, 1)?).to_owned(),
        ))),
        "mediaRecorderOpen" => {
            if !state.document_origin.is_potentially_trustworthy() {
                return Err(JsNativeError::typ()
                    .with_message("Audio recording requires a secure context")
                    .into());
            }
            let id = state
                .media_recorders
                .open(
                    &argument_string(args, 1)?,
                    integer(args, 2, 0.0)?,
                    args.get(3).is_some_and(JsValue::to_boolean),
                )
                .map_err(|message| JsNativeError::range().with_message(message))?;
            Ok(Some(JsValue::from(id)))
        }
        "mediaRecorderFormat" => Ok(Some(JsValue::from(state.media_recorders.format_supported(
            positive_integer(args, 1)?,
            positive_integer(args, 2)? as usize,
            positive_integer(args, 3)? as usize,
        )))),
        "mediaRecorderEncode" => {
            let id = positive_integer(args, 1)?;
            let sample_rate = positive_integer(args, 2)? as usize;
            let channels = positive_integer(args, 3)? as usize;
            let pcm = args.get(4).and_then(JsValue::as_bytes).ok_or_else(|| {
                JsNativeError::typ().with_message("Capture PCM must be a Uint8Array")
            })?;
            let encoded = state
                .media_recorders
                .append(id, sample_rate, channels, pcm)
                .map_err(|message| JsNativeError::range().with_message(message))?;
            Ok(Some(JsValue::Bytes(encoded)))
        }
        "mediaRecorderFinish" => {
            let encoded = state
                .media_recorders
                .finish(positive_integer(args, 1)?)
                .map_err(|message| JsNativeError::range().with_message(message))?;
            Ok(Some(JsValue::Bytes(encoded)))
        }
        "mediaRecorderCancel" => {
            state.media_recorders.cancel(positive_integer(args, 1)?);
            Ok(Some(JsValue::undefined()))
        }
        _ => Ok(None),
    }
}
