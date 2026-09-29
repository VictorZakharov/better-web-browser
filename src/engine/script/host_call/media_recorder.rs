//! Private bridge between live PCM capture and bounded per-document FLAC encoders.

use super::super::*;

fn positive_integer(args: &[JsValue], index: usize) -> JsResult<u32> {
    let value = args
        .get(index)
        .and_then(JsValue::as_number)
        .filter(|value| {
            value.is_finite() && value.fract() == 0.0 && *value >= 1.0 && *value <= u32::MAX as f64
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
        "mediaRecorderOpen" => {
            if !state.document_origin.is_potentially_trustworthy() {
                return Err(JsNativeError::typ()
                    .with_message("Audio recording requires a secure context")
                    .into());
            }
            let id = state
                .media_recorders
                .open()
                .map_err(|message| JsNativeError::range().with_message(message))?;
            Ok(Some(JsValue::from(id)))
        }
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
