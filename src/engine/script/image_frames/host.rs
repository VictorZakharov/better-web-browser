//! Small byte-oriented host protocol, also available in dedicated workers.
use super::{CHUNK_BYTES, ImageFrames, codecs};
use crate::engine::script::{JsNativeError, JsResult, JsValue};

pub(super) fn integer(args: &[JsValue], position: usize) -> JsResult<u32> {
    let number = args
        .get(position)
        .and_then(JsValue::as_number)
        .ok_or_else(|| JsNativeError::typ().with_message("image decoder integer is required"))?;
    if !number.is_finite() || number < 0.0 || number > u32::MAX as f64 || number.fract() != 0.0 {
        return Err(JsNativeError::range()
            .with_message("invalid image decoder integer")
            .into());
    }
    Ok(number as u32)
}

fn object(entries: Vec<(&str, JsValue)>) -> JsValue {
    JsValue::Object(
        entries
            .into_iter()
            .map(|(key, value)| (key.into(), value))
            .collect(),
    )
}

pub(in crate::engine::script) fn dispatch(
    operation: &str,
    args: &[JsValue],
    decoders: &mut ImageFrames,
) -> JsResult<Option<JsValue>> {
    let value = match operation {
        "videoFrameRGB" => return super::pixels::dispatch(args).map(Some),
        "imageDecoderSupported" => JsValue::from(codecs::supported(
            &args.get(1).map(JsValue::string_value).unwrap_or_default(),
        )),
        "imageDecoderStart" => {
            let mime = args.get(1).map(JsValue::string_value).unwrap_or_default();
            let bytes = args.get(2).and_then(JsValue::as_bytes).ok_or_else(|| {
                JsNativeError::typ().with_message("encoded image bytes are required")
            })?;
            let ignore_profile = matches!(args.get(3), Some(JsValue::Boolean(true)));
            let id = decoders
                .start(&mime, bytes, ignore_profile)
                .map_err(|message| JsNativeError::range().with_message(message))?;
            JsValue::from(id)
        }
        "imageDecoderPoll" => match decoders.poll(integer(args, 1)?) {
            Ok(None) => object(vec![("status", JsValue::from("pending".to_owned()))]),
            Err(error) => object(vec![
                ("status", JsValue::from("error".to_owned())),
                ("message", JsValue::from(error)),
            ]),
            Ok(Some(frames)) => object(vec![
                ("status", JsValue::from("ready".to_owned())),
                ("frameCount", JsValue::from(frames.frames.len() as f64)),
                ("animated", JsValue::from(frames.animated)),
                ("poster", JsValue::from(frames.poster.is_some())),
                (
                    "repetitions",
                    frames.repetitions.map_or(JsValue::Null, JsValue::from),
                ),
            ]),
        },
        "imageDecoderFrame" => {
            let id = integer(args, 1)?;
            let index = integer(args, 2)? as usize;
            let offset = integer(args, 3)? as usize;
            let track = integer(args, 4)?;
            let frames = decoders
                .poll(id)
                .map_err(|e| JsNativeError::typ().with_message(e))?
                .ok_or_else(|| JsNativeError::typ().with_message("image metadata is not ready"))?;
            let frame = match (frames.poster.as_ref(), track) {
                (Some(poster), 0) => (index == 0).then_some(poster),
                (Some(_), 1) | (None, 0) => frames.frames.get(index),
                _ => None,
            }
            .ok_or_else(|| {
                JsNativeError::range().with_message("image frame index is out of range")
            })?;
            if offset >= frame.pixels.len() || !offset.is_multiple_of(CHUNK_BYTES) {
                return Err(JsNativeError::range()
                    .with_message("invalid image frame byte offset")
                    .into());
            }
            let end = (offset + CHUNK_BYTES).min(frame.pixels.len());
            object(vec![
                ("width", JsValue::from(frame.width)),
                ("height", JsValue::from(frame.height)),
                ("timestamp", JsValue::from(frame.timestamp as f64)),
                (
                    "duration",
                    frame
                        .duration
                        .map_or(JsValue::Null, |v| JsValue::from(v as f64)),
                ),
                ("rotation", JsValue::from(u32::from(frame.rotation))),
                ("flip", JsValue::from(frame.flip)),
                ("offset", JsValue::from(offset as f64)),
                ("done", JsValue::from(end == frame.pixels.len())),
                ("pixels", JsValue::Bytes(frame.pixels[offset..end].to_vec())),
            ])
        }
        "imageDecoderClose" => {
            decoders.close(integer(args, 1)?);
            JsValue::undefined()
        }
        _ => return Ok(None),
    };
    Ok(Some(value))
}
