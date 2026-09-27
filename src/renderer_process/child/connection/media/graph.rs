use super::*;
use crate::media_protocol::{GraphPcmFormat, GraphPcmStatus};
use crate::renderer_protocol::DocumentId;
use std::time::{Duration, Instant};

// Three waiting chunks plus at most one chunk retrying after worker backpressure.
const MAX_PENDING_GRAPH_CHUNKS: usize = 3;
const GRAPH_PCM_STALL_TIMEOUT: Duration = Duration::from_secs(5);

enum GraphCommand {
    Queue {
        document_id: u64,
        context_id: u64,
        generation: u64,
        format: GraphPcmFormat,
        pcm: Vec<u8>,
    },
    Wake,
}

pub(super) struct GraphPcmPump {
    commands: mpsc::SyncSender<GraphCommand>,
    state: Arc<Mutex<GraphState>>,
    fault: Arc<Mutex<Option<String>>>,
}

#[derive(Clone, Copy)]
struct GraphState {
    document_id: u64,
    context_id: u64,
    last_context_id: u64,
    generation: u64,
}

impl GraphState {
    fn advance(&mut self) {
        // Zero disables admission rather than allowing a wrapped epoch to reuse stale commands.
        self.generation = self.generation.checked_add(1).unwrap_or(0);
    }
}

impl GraphPcmPump {
    fn new(
        client: Arc<Mutex<crate::media_process::MediaClient>>,
        document_id: u64,
    ) -> Result<Self, String> {
        let (commands, receiver) = mpsc::sync_channel(MAX_PENDING_GRAPH_CHUNKS);
        let state = Arc::new(Mutex::new(GraphState {
            document_id,
            context_id: 0,
            last_context_id: 0,
            generation: 1,
        }));
        let fault = Arc::new(Mutex::new(None));
        let thread_state = Arc::clone(&state);
        let thread_fault = Arc::clone(&fault);
        std::thread::Builder::new()
            .name("breeze-renderer-graph-pcm".into())
            .spawn(move || run(client, receiver, thread_state, thread_fault))
            .map_err(|error| format!("start graph PCM transport: {error}"))?;
        Ok(Self {
            commands,
            state,
            fault,
        })
    }

    fn activate(&self, document_id: u64) {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        state.advance();
        state.document_id = document_id;
        state.context_id = 0;
        state.last_context_id = 0;
        if let Ok(mut fault) = self.fault.lock() {
            *fault = None;
        }
        drop(state);
        let _ = self.commands.try_send(GraphCommand::Wake);
    }

    fn retire(&self, document_id: u64) {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        if state.document_id == document_id {
            state.advance();
            state.document_id = 0;
            state.context_id = 0;
            drop(state);
            // A full queue already wakes the thread; it will discard stale chunks before closing.
            let _ = self.commands.try_send(GraphCommand::Wake);
        }
    }

    fn close(&self, document_id: u64, context_id: u64) -> bool {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        if context_id == 0 || state.document_id != document_id || state.context_id != context_id {
            return false;
        }
        state.advance();
        state.context_id = 0;
        drop(state);
        let _ = self.commands.try_send(GraphCommand::Wake);
        true
    }

    fn try_queue(
        &self,
        document_id: u64,
        context_id: u64,
        format: GraphPcmFormat,
        pcm: Vec<u8>,
    ) -> Result<bool, String> {
        format
            .validate(pcm.len())
            .map_err(|error| error.to_string())?;
        let mut state = self
            .state
            .lock()
            .map_err(|_| "graph PCM state lock was poisoned")?;
        if document_id == 0
            || context_id == 0
            || state.generation == 0
            || state.document_id != document_id
        {
            return Ok(false);
        }
        let first_chunk = state.context_id == 0;
        if (first_chunk && context_id <= state.last_context_id)
            || (!first_chunk && state.context_id != context_id)
        {
            return Ok(false);
        }
        if let Some(error) = self
            .fault
            .lock()
            .map_err(|_| "graph PCM status lock was poisoned")?
            .as_ref()
        {
            return Err(error.clone());
        }
        match self.commands.try_send(GraphCommand::Queue {
            document_id,
            context_id,
            generation: state.generation,
            format,
            pcm,
        }) {
            Ok(()) => {
                if first_chunk {
                    state.context_id = context_id;
                    state.last_context_id = context_id;
                }
                Ok(true)
            }
            Err(mpsc::TrySendError::Full(_)) => Ok(false),
            Err(mpsc::TrySendError::Disconnected(_)) => {
                Err("graph PCM transport disconnected".into())
            }
        }
    }
}

impl Drop for GraphPcmPump {
    fn drop(&mut self) {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        state.advance();
        state.document_id = 0;
        state.context_id = 0;
        drop(state);
        let _ = self.commands.try_send(GraphCommand::Wake);
    }
}

fn run(
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
        let result = retry_backpressure(
            || {
                with_client(&client, |client| {
                    client.queue_graph_pcm(document_id, context_id, format, pcm.clone())
                })
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
                active = Some((document_id, context_id, command_generation))
            }
            Ok(None) => {}
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

fn retry_backpressure(
    mut submit: impl FnMut() -> Result<GraphPcmStatus, String>,
    mut still_current: impl FnMut() -> bool,
    mut wait: impl FnMut(),
    timeout: Duration,
) -> Result<Option<GraphPcmStatus>, String> {
    let started = Instant::now();
    while still_current() {
        match submit()? {
            GraphPcmStatus::Backpressure if started.elapsed() < timeout => wait(),
            GraphPcmStatus::Backpressure => {
                return Err("graph PCM output remained backpressured".into());
            }
            status => return Ok(Some(status)),
        }
    }
    Ok(None)
}

fn with_client<T>(
    client: &Arc<Mutex<crate::media_process::MediaClient>>,
    operation: impl FnOnce(&mut crate::media_process::MediaClient) -> Result<T, String>,
) -> Result<T, String> {
    let mut client = client
        .lock()
        .map_err(|_| "contained media client lock was poisoned")?;
    operation(&mut client)
}

fn record_fault_if_current(
    state: &Mutex<GraphState>,
    fault: &Mutex<Option<String>>,
    document_id: u64,
    context_id: u64,
    generation: u64,
    error: String,
) {
    let current = state.lock().unwrap_or_else(|poison| poison.into_inner());
    if current.document_id == document_id
        && current.context_id == context_id
        && current.generation == generation
        && let Ok(mut slot) = fault.lock()
    {
        *slot = Some(error);
    }
}

impl ChildConnection {
    pub(in crate::renderer_process::child) fn activate_graph_pcm(&mut self, document: DocumentId) {
        if let Some(media) = self.media.as_mut() {
            media.graph_document = document.get();
            if let Some(graph) = media.graph.as_ref() {
                graph.activate(document.get());
            }
        }
    }

    pub(in crate::renderer_process::child) fn retire_graph_pcm(&mut self, document: DocumentId) {
        if let Some(media) = self
            .media
            .as_mut()
            .filter(|media| media.graph_document == document.get())
        {
            media.graph_document = 0;
            if let Some(graph) = media.graph.as_ref() {
                graph.retire(document.get());
            }
        }
    }

    pub(in crate::renderer_process::child) fn retire_all_graph_pcm(&mut self) {
        if let Some(media) = self.media.as_mut() {
            let document_id = std::mem::take(&mut media.graph_document);
            if let Some(graph) = media.graph.as_ref() {
                graph.retire(document_id);
            }
        }
    }

    /// The realtime graph is not page-visible yet; the eventual host may call this without
    /// waiting for media IPC or an audio device. False means bounded queue backpressure.
    #[allow(dead_code)]
    pub(in crate::renderer_process::child) fn try_queue_graph_pcm(
        &mut self,
        document: DocumentId,
        context_id: u64,
        format: GraphPcmFormat,
        pcm: Vec<u8>,
    ) -> Result<bool, String> {
        let media = self
            .media
            .as_mut()
            .ok_or("contained media worker is unavailable")?;
        if media.graph_document != document.get() {
            return Ok(false);
        }
        if media.graph.is_none() {
            media.graph = Some(GraphPcmPump::new(
                Arc::clone(&media.client),
                document.get(),
            )?);
        }
        media
            .graph
            .as_ref()
            .expect("graph PCM pump initialized")
            .try_queue(document.get(), context_id, format, pcm)
    }

    #[allow(dead_code)]
    pub(in crate::renderer_process::child) fn close_graph_pcm(
        &self,
        document: DocumentId,
        context_id: u64,
    ) -> bool {
        self.media
            .as_ref()
            .filter(|media| media.graph_document == document.get())
            .and_then(|media| media.graph.as_ref())
            .is_some_and(|graph| graph.close(document.get(), context_id))
    }
}

#[cfg(test)]
mod tests;
