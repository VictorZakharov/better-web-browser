use super::*;

pub(super) fn dispatch(
    operation: &str,
    args: &[JsValue],
    state: &mut HostState,
) -> JsResult<Option<JsValue>> {
    let reply = match operation {
        "audioRenderAvailable" => JsValue::from(state.audio_render_thread_available),
        "audioRenderStart" => {
            if state.embedded || !state.audio_activated {
                return Ok(Some(JsValue::from(false)));
            }
            let stream_id = positive_integer(args, 1, "stream ID")?;
            let rate = positive_integer(args, 2, "sample rate")?;
            GraphPcmFormat {
                sample_rate: rate,
                channels: 2,
            }
            .validate(4)
            .map_err(|_| JsNativeError::range().with_message("invalid rendering sample rate"))?;
            if state.audio_renderer.is_none() {
                state.audio_renderer = Some(
                    super::super::super::audio_render::AudioRenderControl::new()
                        .map_err(|error| JsNativeError::error().with_message(error))?,
                );
            }
            let renderer = state
                .audio_renderer
                .as_ref()
                .expect("audio renderer initialized");
            renderer.start(stream_id, rate);
            state
                .pending_graph_audio_actions
                .push(ScriptGraphAudioAction::Render {
                    stream_id,
                    renderer: renderer.clone(),
                });
            JsValue::from(true)
        }
        "audioRenderAsset" => {
            let id = positive_integer(args, 1, "audio resource ID")?;
            let description = args.get(2).map(JsValue::string_value).unwrap_or_default();
            let Some(JsValue::Array(arrays)) = args.get(3) else {
                return Err(JsNativeError::typ()
                    .with_message("audio resource needs binary channels")
                    .into());
            };
            if description.len() > 16 * 1024 * 1024 || arrays.iter().any(|v| v.as_bytes().is_none())
            {
                return Err(JsNativeError::range()
                    .with_message("invalid audio resource transfer")
                    .into());
            }
            JsValue::from(
                state
                    .audio_renderer
                    .as_ref()
                    .is_some_and(|renderer| renderer.asset(id, description, arrays.clone())),
            )
        }
        "audioRenderGraph" => {
            JsValue::from(state.audio_renderer.as_ref().is_some_and(|renderer| {
                renderer.graph(args.get(1).map(JsValue::string_value).unwrap_or_default())
            }))
        }
        "audioRenderClock" => {
            JsValue::from(state.audio_renderer.as_ref().map_or(0, |r| r.frame()) as f64)
        }
        "audioRenderFeedback" => {
            let id = positive_integer(args, 1, "audio node ID")?;
            state
                .audio_renderer
                .as_ref()
                .map_or(JsValue::Null, |r| r.feedback(id))
        }
        "audioRenderPause" => {
            if let Some(renderer) = state.audio_renderer.as_ref() {
                renderer.pause();
            }
            if args.get(1).is_some_and(JsValue::to_boolean) {
                state.audio_renderer = None;
            }
            JsValue::Undefined
        }
        "audioRenderPoll" => state
            .audio_renderer
            .as_ref()
            .map_or(JsValue::Null, |renderer| renderer.poll()),
        "audioRenderCallback" => JsValue::from(
            state
                .audio_renderer
                .as_ref()
                .is_some_and(|renderer| renderer.callback(args.iter().skip(1).cloned().collect())),
        ),
        _ => return Ok(None),
    };
    Ok(Some(reply))
}
