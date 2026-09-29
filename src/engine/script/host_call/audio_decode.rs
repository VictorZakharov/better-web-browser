//! Private, bounded bridge for off-thread Web Audio file decoding.
use super::super::*;
use crate::engine::script::audio_decode::Poll;

fn request_id(args: &[JsValue]) -> JsResult<u32> {
    let Some(number) = args.get(1).and_then(JsValue::as_number) else {
        return Err(JsNativeError::typ()
            .with_message("audio decode ID is required")
            .into());
    };
    if !number.is_finite() || number.fract() != 0.0 || number < 1.0 || number > u32::MAX as f64 {
        return Err(JsNativeError::range()
            .with_message("audio decode ID is invalid")
            .into());
    }
    Ok(number as u32)
}

pub(super) fn dispatch(
    operation: &str,
    args: &[JsValue],
    state: &mut HostState,
) -> JsResult<Option<JsValue>> {
    match operation {
        "audioDecodeStart" => {
            let bytes = args.get(1).and_then(JsValue::as_bytes).ok_or_else(|| {
                JsNativeError::typ().with_message("encoded audio must be a Uint8Array")
            })?;
            let rate = args.get(2).and_then(JsValue::as_number).ok_or_else(|| {
                JsNativeError::typ().with_message("context sample rate is required")
            })?;
            let id = state
                .audio_decodes
                .start(bytes.to_vec(), rate)
                .map_err(|message| JsNativeError::range().with_message(message))?;
            Ok(Some(JsValue::from(id)))
        }
        "audioDecodePoll" => {
            let result = match state.audio_decodes.poll(request_id(args)?) {
                Poll::Pending => {
                    JsValue::Object(vec![("status".into(), JsValue::from("pending".to_owned()))])
                }
                Poll::Error(message) => JsValue::Object(vec![
                    ("status".into(), JsValue::from("error".to_owned())),
                    ("message".into(), JsValue::from(message)),
                ]),
                Poll::Data {
                    channels,
                    frames,
                    sample_rate,
                    channel,
                    offset,
                    bytes,
                    done,
                } => JsValue::Object(vec![
                    ("status".into(), JsValue::from("data".to_owned())),
                    ("channels".into(), JsValue::from(channels as f64)),
                    ("frames".into(), JsValue::from(frames as f64)),
                    ("sampleRate".into(), JsValue::from(sample_rate)),
                    ("channel".into(), JsValue::from(channel as f64)),
                    ("offset".into(), JsValue::from(offset as f64)),
                    ("bytes".into(), JsValue::Bytes(bytes)),
                    ("done".into(), JsValue::from(done)),
                ]),
            };
            Ok(Some(result))
        }
        "audioDecodeCancel" => {
            state.audio_decodes.cancel(request_id(args)?);
            Ok(Some(JsValue::undefined()))
        }
        _ => Ok(None),
    }
}
