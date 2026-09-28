//! Bounded private bridge from a document's Web Audio graph to the contained media worker.

use super::super::*;
use crate::media_protocol::GraphPcmFormat;

fn positive_integer(args: &[JsValue], index: usize, name: &str) -> JsResult<u32> {
    let value = args
        .get(index)
        .and_then(JsValue::as_number)
        .ok_or_else(|| JsNativeError::typ().with_message(format!("{name} must be a number")))?;
    if !value.is_finite() || value.fract() != 0.0 || value < 1.0 || value > u32::MAX as f64 {
        return Err(JsNativeError::range()
            .with_message(format!("{name} is out of range"))
            .into());
    }
    Ok(value as u32)
}

pub(super) fn dispatch(
    operation: &str,
    args: &[JsValue],
    state: &mut HostState,
) -> JsResult<Option<JsValue>> {
    match operation {
        "audioContextAllowed" => Ok(Some(JsValue::from(
            state.audio_activated && !state.embedded,
        ))),
        "audioGraphQueue" => {
            if state.embedded || !state.audio_activated {
                return Ok(Some(JsValue::from(false)));
            }
            let stream_id = positive_integer(args, 1, "stream ID")?;
            let format = GraphPcmFormat {
                sample_rate: positive_integer(args, 2, "sample rate")?,
                channels: u16::try_from(positive_integer(args, 3, "channel count")?).map_err(
                    |_| JsNativeError::range().with_message("channel count is out of range"),
                )?,
            };
            let pcm = args.get(4).and_then(JsValue::as_bytes).ok_or_else(|| {
                JsNativeError::typ().with_message("graph PCM must be a typed array")
            })?;
            format.validate(pcm.len()).map_err(|_| {
                JsNativeError::range().with_message("graph PCM format or byte count is invalid")
            })?;
            // A single in-flight chunk is enough: the renderer owns retries after this handoff.
            // This also bounds memory before ScriptOutcome reaches the renderer admission path.
            if state
                .pending_graph_audio_actions
                .iter()
                .any(|action| matches!(action, ScriptGraphAudioAction::Queue { .. }))
            {
                return Ok(Some(JsValue::from(false)));
            }
            state
                .pending_graph_audio_actions
                .push(ScriptGraphAudioAction::Queue {
                    stream_id,
                    format,
                    pcm: pcm.to_vec(),
                });
            Ok(Some(JsValue::from(true)))
        }
        "audioGraphClose" => {
            let stream_id = positive_integer(args, 1, "stream ID")?;
            if state.pending_graph_audio_actions.iter().any(|action| {
                matches!(action, ScriptGraphAudioAction::Close { stream_id: id } if *id == stream_id)
            }) {
                return Ok(Some(JsValue::from(true)));
            }
            if state
                .pending_graph_audio_actions
                .iter()
                .any(|action| matches!(action, ScriptGraphAudioAction::Close { .. }))
            {
                return Ok(Some(JsValue::from(false)));
            }
            state
                .pending_graph_audio_actions
                .push(ScriptGraphAudioAction::Close { stream_id });
            Ok(Some(JsValue::from(true)))
        }
        _ => Ok(None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn graph_pcm_host_admits_one_bounded_chunk_and_a_priority_close() {
        let dom = crate::engine::dom::parse_with_scripting("<body></body>", true);
        let runtime = ScriptRuntime::new(dom.document, "https://example.test/");
        let mut host = runtime.host.borrow_mut();
        assert!(
            !dispatch("audioContextAllowed", &[], &mut host)
                .unwrap()
                .unwrap()
                .to_boolean()
        );
        host.audio_activated = true;
        assert!(
            dispatch("audioContextAllowed", &[], &mut host)
                .unwrap()
                .unwrap()
                .to_boolean()
        );

        // Two nonzero signed samples make the exact PCM payload observable here, before the
        // renderer takes ownership and sends it to the media worker's bounded transport.
        let mut pcm = vec![0; 128 * 2 * 2];
        pcm[0..4].copy_from_slice(&[0xff, 0x7f, 0x00, 0x80]);
        let queue = vec![
            JsValue::from("audioGraphQueue".to_owned()),
            JsValue::from(7_u32),
            JsValue::from(48_000_u32),
            JsValue::from(2_u32),
            JsValue::Bytes(pcm.clone()),
        ];
        assert!(
            dispatch("audioGraphQueue", &queue, &mut host)
                .unwrap()
                .unwrap()
                .to_boolean()
        );
        assert!(
            !dispatch("audioGraphQueue", &queue, &mut host)
                .unwrap()
                .unwrap()
                .to_boolean()
        );
        assert!(matches!(
            &host.pending_graph_audio_actions[0],
            ScriptGraphAudioAction::Queue { stream_id: 7, format, pcm: queued }
                if *format == GraphPcmFormat { sample_rate: 48_000, channels: 2 }
                    && queued == &pcm
        ));

        let close = [
            JsValue::from("audioGraphClose".to_owned()),
            JsValue::from(7_u32),
        ];
        assert!(
            dispatch("audioGraphClose", &close, &mut host)
                .unwrap()
                .unwrap()
                .to_boolean()
        );
        assert!(matches!(
            host.pending_graph_audio_actions[1],
            ScriptGraphAudioAction::Close { stream_id: 7 }
        ));

        let mut oversized = queue;
        oversized[4] = JsValue::Bytes(vec![0; 897 * 2 * 2]);
        assert!(dispatch("audioGraphQueue", &oversized, &mut host).is_err());
    }
}
