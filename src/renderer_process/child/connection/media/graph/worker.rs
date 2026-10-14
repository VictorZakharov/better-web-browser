use super::*;

pub(super) fn run(
    client: Arc<Mutex<crate::media_process::MediaClient>>,
    receiver: mpsc::Receiver<GraphCommand>,
    state: Arc<Mutex<GraphState>>,
    fault: Arc<Mutex<Option<String>>>,
) {
    let mut active: Option<(u64, u64, u64)> = None;
    while let Ok(command) = receiver.recv() {
        let current = *state.lock().unwrap_or_else(|poison| poison.into_inner());
        if let Some((old_document, old_context, _)) = active.filter(|(id, context, epoch)| {
            *id != current.document_id
                || *context != current.context_id
                || *epoch != current.generation
        }) {
            let _ = with_client(&client, |client| {
                client.close_graph_pcm(old_document, old_context)
            });
            active = None;
        }
        // The worker has observed this generation only after a prior graph voice was
        // released. A document may settle close() when its ticket has been observed.
        let mut latest = state.lock().unwrap_or_else(|poison| poison.into_inner());
        if latest.generation == current.generation {
            latest.worker_generation_seen = current.generation;
        }
        drop(latest);
        let GraphCommand::Queue {
            document_id,
            context_id,
            generation: command_generation,
            format,
            pcm,
        } = command
        else {
            continue;
        };
        if document_id != current.document_id
            || context_id != current.context_id
            || command_generation != current.generation
        {
            continue;
        }
        let mut health = GraphPcmHealth::default();
        let result = retry_backpressure(
            || {
                let reply = with_client(&client, |client| {
                    client.queue_graph_pcm(document_id, context_id, format, pcm.clone())
                })?;
                health = reply.1;
                Ok(reply.0)
            },
            || {
                let current = *state.lock().unwrap_or_else(|poison| poison.into_inner());
                current.document_id == document_id
                    && current.context_id == context_id
                    && current.generation == command_generation
            },
            || std::thread::sleep(Duration::from_millis(10)),
            GRAPH_PCM_STALL_TIMEOUT,
        );
        match result {
            Ok(Some(GraphPcmStatus::Accepted)) => {
                let mut latest = state.lock().unwrap_or_else(|poison| poison.into_inner());
                if latest.document_id == document_id
                    && latest.context_id == context_id
                    && latest.generation == command_generation
                {
                    latest.worker_accepted_generation = command_generation;
                    latest.health = health;
                    latest.health_observed = Instant::now();
                    latest.sample_rate = format.sample_rate;
                    active = Some((document_id, context_id, command_generation));
                } else {
                    drop(latest);
                    let _ = with_client(&client, |client| {
                        client.close_graph_pcm(document_id, context_id)
                    });
                }
            }
            Ok(None) => {
                // Invalidation can race a successful IPC response. The worker may
                // have created a voice although the retry loop now sees a stale
                // generation, so close that exact identity before acknowledging it.
                let _ = with_client(&client, |client| {
                    client.close_graph_pcm(document_id, context_id)
                });
            }
            Ok(Some(GraphPcmStatus::Backpressure)) => unreachable!(),
            terminal => {
                let error = match terminal {
                    Ok(Some(GraphPcmStatus::Rejected)) => {
                        "graph PCM stream was rejected by the media worker".into()
                    }
                    Err(error) => error,
                    _ => unreachable!(),
                };
                record_fault_if_current(
                    &state,
                    &fault,
                    document_id,
                    context_id,
                    command_generation,
                    error,
                );
                // A lost reply may follow successful device admission. Close by identity even
                // when acceptance is unknown; this never affects decoded-media playback.
                let _ = with_client(&client, |client| {
                    client.close_graph_pcm(document_id, context_id)
                });
                active = None;
            }
        }
    }
    if let Some((document_id, context_id, _)) = active {
        let _ = with_client(&client, |client| {
            client.close_graph_pcm(document_id, context_id)
        });
    }
}
