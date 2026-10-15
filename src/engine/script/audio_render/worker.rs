use super::*;

struct Stream {
    id: u32,
    output: Arc<dyn AudioRenderOutput>,
    admitted: u64,
}

impl Drop for Stream {
    fn drop(&mut self) {
        self.output.close(self.id);
    }
}

pub(super) fn run(
    shared: &Mutex<State>,
    clock: &AtomicU64,
    wake: mpsc::Receiver<()>,
) -> Result<(), String> {
    let mut context =
        Context::new(HostBridge::Worker(Default::default())).map_err(|error| error.to_string())?;
    context
        .eval(Source::from_bytes(bootstrap::BOOTSTRAP))
        .map_err(|error| error.to_string())?;
    let mut initialized = false;
    let mut stream: Option<Stream> = None;
    let mut pending: Option<Vec<u8>> = None;
    loop {
        let (requested, rate, output, graph, assets, callbacks) = {
            let mut state = shared
                .lock()
                .map_err(|_| "audio rendering control lock was poisoned")?;
            state.transfer_bytes = state
                .uncommitted_assets
                .iter()
                .map(|asset| {
                    asset.description.len()
                        + asset
                            .arrays
                            .iter()
                            .filter_map(JsValue::as_bytes)
                            .map(<[u8]>::len)
                            .sum::<usize>()
                })
                .sum();
            (
                state.stream,
                state.rate,
                state.output.clone(),
                state.graph.take(),
                std::mem::take(&mut state.assets),
                std::mem::take(&mut state.callbacks),
            )
        };
        if stream.as_ref().map(|s| s.id) != requested.map(|s| s.0) {
            stream = None;
            if let (Some((id, _)), Some(output)) = (requested, output) {
                stream = Some(Stream {
                    id,
                    output,
                    admitted: 0,
                });
            }
        }
        if !initialized && rate != 0 {
            call(&mut context, "__audioThreadInit", &[JsValue::from(rate)])?;
            initialized = true;
        }
        for asset in assets {
            call(
                &mut context,
                "__audioThreadAsset",
                &[
                    JsValue::from(asset.id),
                    JsValue::from(asset.description),
                    JsValue::Array(asset.arrays),
                ],
            )?;
        }
        if let Some(graph) = graph {
            call(&mut context, "__audioThreadGraph", &[JsValue::from(graph)])?;
        }
        for callback in callbacks {
            call(&mut context, "__audioThreadCallback", &callback)?;
        }
        let mut wait = Duration::from_millis(5);
        if let (Some(current), Some((_, rate))) = (&mut stream, requested) {
            let queued = current.output.health(current.id).map_or(0, |health| {
                current.admitted.saturating_sub(health.played_frames)
            });
            if pending.is_none() && queued < u64::from(rate) * 120 / 1000 {
                let pcm = call(&mut context, "__audioThreadRender", &[])?;
                pending = Some(
                    pcm.as_bytes()
                        .ok_or("audio renderer returned invalid PCM")?
                        .to_vec(),
                );
                let frame = call(&mut context, "__audioThreadFrame", &[])?;
                clock.store(
                    frame
                        .as_number()
                        .ok_or("audio renderer returned invalid clock")? as u64,
                    Ordering::Release,
                );
                let events = call(&mut context, "__audioThreadEvents", &[])?;
                if let JsValue::Array(parts) = &events {
                    let bytes = parts
                        .iter()
                        .filter_map(JsValue::as_bytes)
                        .map(<[u8]>::len)
                        .sum::<usize>();
                    let mut state = shared
                        .lock()
                        .map_err(|_| "audio rendering event lock was poisoned")?;
                    if state.packets.len() >= 4096
                        || bytes > (32 * 1024 * 1024_usize).saturating_sub(state.packet_bytes)
                    {
                        return Err(
                            "audio rendering event queue exceeded its bounded budget".into()
                        );
                    }
                    if matches!(parts.first(), Some(JsValue::String(s)) if s != "{\"events\":[],\"retired\":[]}")
                    {
                        state.packet_bytes += bytes;
                        state.packets.push(events);
                    }
                }
                let requests = shared
                    .lock()
                    .map_err(|_| "audio feedback lock was poisoned")?
                    .feedback_requests
                    .iter()
                    .copied()
                    .map(JsValue::from)
                    .collect::<Vec<_>>();
                if !requests.is_empty() {
                    let replies = call(
                        &mut context,
                        "__audioThreadFeedback",
                        &[JsValue::Array(requests)],
                    )?;
                    if let JsValue::Array(replies) = replies {
                        let mut state = shared
                            .lock()
                            .map_err(|_| "audio feedback lock was poisoned")?;
                        for reply in replies {
                            if let JsValue::Array(mut values) = reply
                                && let Some(id) = values.first().and_then(JsValue::as_number)
                            {
                                let id = id as u32;
                                if values.len() > 1 {
                                    values.remove(0);
                                    state.feedback.insert(id, JsValue::Array(values));
                                } else {
                                    state.feedback_requests.remove(&id);
                                    state.feedback.remove(&id);
                                }
                            }
                        }
                    }
                }
            }
            if let Some(pcm) = pending.as_ref() {
                let format = GraphPcmFormat {
                    sample_rate: rate,
                    channels: 2,
                };
                let accepted = current.output.queue(current.id, format, pcm.clone());
                let current_request = shared
                    .lock()
                    .map_err(|_| "audio rendering control lock was poisoned")?
                    .stream;
                let accepted = match accepted {
                    Err(_) if current_request.map(|s| s.0) != Some(current.id) => false,
                    result => result?,
                };
                if accepted {
                    current.admitted += pcm.len() as u64 / 4;
                    pending = None;
                    wait = Duration::ZERO;
                }
            }
            if current.admitted != 0 {
                let started = current.output.started(current.id);
                let mut state = shared
                    .lock()
                    .map_err(|_| "audio rendering status lock was poisoned")?;
                if state.stream.map(|s| s.0) == Some(current.id) {
                    state.started = started?;
                }
            }
        }
        match wake.recv_timeout(wait) {
            Ok(()) | Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => return Ok(()),
        }
    }
}

fn call(context: &mut Context, name: &str, arguments: &[JsValue]) -> Result<JsValue, String> {
    context
        .call_global(name, arguments)
        .map_err(|error| format!("Web Audio rendering {name}: {error}"))
}
