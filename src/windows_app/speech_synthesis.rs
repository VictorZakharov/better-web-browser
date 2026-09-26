//! Browser-owned speech synthesis. Page code never receives a COM interface or audio handle.

mod dispatch;
mod sapi;

use self::sapi::SapiVoice;
use super::tabs::TabId;
use better_web_browser::renderer_process::SpeechUpdateSink;
use better_web_browser::renderer_protocol::{
    DocumentId, SpeechAction, SpeechEvent, SpeechRequest, SpeechUpdate, SpeechVoiceInfo,
};
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

const COMMAND_CAPACITY: usize = 32;
const MAX_QUEUED_UTTERANCES: usize = 32;
const MAX_ACTIVE_OWNERS: usize = 16;
const POLL_INTERVAL: Duration = Duration::from_millis(25);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) struct SpeechOwner {
    pub tab: TabId,
    pub document: DocumentId,
    pub session_id: u64,
}

enum WorkerCommand {
    Request(SpeechOwner, SpeechRequest, SpeechUpdateSink),
    Retire(SpeechOwner),
    Shutdown,
}

struct PendingUtterance {
    request: SpeechRequest,
    sink: SpeechUpdateSink,
}

#[derive(Default)]
struct OwnerSpeech {
    voice: Option<Result<SapiVoice, String>>,
    queue: VecDeque<PendingUtterance>,
    speaking: Option<PendingUtterance>,
    paused: bool,
}

pub(super) struct SpeechSynthesisService {
    commands: SyncSender<WorkerCommand>,
    retired: Arc<Mutex<HashMap<TabId, (u64, u64)>>>,
    stopping: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}

impl SpeechSynthesisService {
    pub(super) fn spawn() -> Result<Self, String> {
        let (commands, incoming) = mpsc::sync_channel(COMMAND_CAPACITY);
        let retired = Arc::new(Mutex::new(HashMap::new()));
        let worker_retired = Arc::clone(&retired);
        let stopping = Arc::new(AtomicBool::new(false));
        let worker_stopping = Arc::clone(&stopping);
        let worker = thread::Builder::new()
            .name("breeze-speech".into())
            .spawn(move || run_worker(incoming, worker_retired, worker_stopping))
            .map_err(|error| format!("start speech worker: {error}"))?;
        Ok(Self {
            commands,
            retired,
            stopping,
            worker: Some(worker),
        })
    }

    pub(super) fn send(
        &self,
        owner: SpeechOwner,
        request: SpeechRequest,
        sink: SpeechUpdateSink,
    ) -> Result<(), String> {
        request.validate().map_err(|error| error.to_string())?;
        if request.document != owner.document {
            return Err("speech request document mismatch".into());
        }
        if is_retired(&self.retired, owner) {
            return Err("speech document has retired".into());
        }
        self.commands
            .try_send(WorkerCommand::Request(owner, request, sink))
            .map_err(|error| format!("speech worker command queue: {error}"))
    }

    pub(super) fn retire(&self, owner: SpeechOwner) {
        let mut retired = self
            .retired
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        retired
            .entry(owner.tab)
            .and_modify(|latest| *latest = (*latest).max((owner.session_id, owner.document.get())))
            .or_insert((owner.session_id, owner.document.get()));
        drop(retired);
        let _ = self.commands.try_send(WorkerCommand::Retire(owner));
    }
}

impl Drop for SpeechSynthesisService {
    fn drop(&mut self) {
        // Even a saturated mailbox cannot postpone stopping the native voice.
        self.stopping.store(true, Ordering::Release);
        let _ = self.commands.try_send(WorkerCommand::Shutdown);
        // The thread owns the COM apartment. Avoid blocking browser shutdown on a
        // third-party voice engine that failed to return from a SAPI call.
        self.worker.take();
    }
}

fn run_worker(
    incoming: Receiver<WorkerCommand>,
    retired: Arc<Mutex<HashMap<TabId, (u64, u64)>>>,
    stopping: Arc<AtomicBool>,
) {
    // Each document has independent queue, pause state, and SAPI voice. A paused
    // tab must not stall another tab; COM remains confined to this worker thread.
    // Idle browsing opens no native voice, and idle owner states are discarded.
    let mut owners = HashMap::<SpeechOwner, OwnerSpeech>::new();
    let mut voice_catalog: Option<Result<Vec<SpeechVoiceInfo>, String>> = None;
    loop {
        if stopping.load(Ordering::Acquire) {
            break;
        }
        owners.retain(|owner, state| {
            if is_retired(&retired, *owner) {
                cancel_owner(*owner, state, true);
                false
            } else {
                true
            }
        });
        match incoming.recv_timeout(POLL_INTERVAL) {
            Ok(WorkerCommand::Request(owner, request, sink)) if is_retired(&retired, owner) => {
                if request.utterance_id != 0 {
                    emit(&sink, owner, request.utterance_id, SpeechEvent::Cancelled);
                }
            }
            Ok(WorkerCommand::Request(owner, request, sink)) => match &request.action {
                SpeechAction::GetVoices => {
                    // One lazy enumeration per browser process also bounds work
                    // from scripts that repeatedly call getVoices().
                    let event = match voice_catalog
                        .get_or_insert_with(|| SapiVoice::open().map(|voice| voice.voices()))
                    {
                        Ok(voices) => SpeechEvent::Voices(voices.clone()),
                        Err(error) => SpeechEvent::Error(error.clone()),
                    };
                    emit(&sink, owner, request.utterance_id, event);
                }
                SpeechAction::Speak { .. } => {
                    if !owners.contains_key(&owner) && owners.len() >= MAX_ACTIVE_OWNERS {
                        emit(
                            &sink,
                            owner,
                            request.utterance_id,
                            SpeechEvent::Error("too many active speech documents".into()),
                        );
                    } else if owners.entry(owner).or_default().queue.len() >= MAX_QUEUED_UTTERANCES
                    {
                        emit(
                            &sink,
                            owner,
                            request.utterance_id,
                            SpeechEvent::Error("speech queue is full".into()),
                        );
                    } else {
                        owners
                            .entry(owner)
                            .or_default()
                            .queue
                            .push_back(PendingUtterance { request, sink });
                    }
                }
                SpeechAction::Pause => {
                    if owners.contains_key(&owner) || owners.len() < MAX_ACTIVE_OWNERS {
                        pause_owner(owner, owners.entry(owner).or_default());
                    }
                }
                SpeechAction::Resume => {
                    if let Some(state) = owners.get_mut(&owner) {
                        resume_owner(owner, state);
                    }
                }
                SpeechAction::Cancel => {
                    if let Some(state) = owners.get_mut(&owner) {
                        cancel_owner(owner, state, false);
                    }
                }
            },
            Ok(WorkerCommand::Retire(owner)) => {
                if let Some(mut state) = owners.remove(&owner) {
                    cancel_owner(owner, &mut state, true);
                }
            }
            Ok(WorkerCommand::Shutdown) | Err(mpsc::RecvTimeoutError::Disconnected) => break,
            Err(mpsc::RecvTimeoutError::Timeout) => {}
        }
        for (owner, state) in &mut owners {
            advance_owner(*owner, state);
        }
        owners
            .retain(|_, state| state.paused || state.speaking.is_some() || !state.queue.is_empty());
    }
    for (owner, state) in &mut owners {
        cancel_owner(*owner, state, true);
    }
}

fn pause_owner(owner: SpeechOwner, state: &mut OwnerSpeech) {
    if state.paused {
        return;
    }
    state.paused = true;
    if let Some(current) = state.speaking.as_ref()
        && let Some(Ok(voice)) = state.voice.as_mut()
    {
        match voice.pause() {
            Ok(()) => emit(
                &current.sink,
                owner,
                current.request.utterance_id,
                SpeechEvent::Paused,
            ),
            Err(error) => {
                let _ = voice.cancel();
                emit(
                    &current.sink,
                    owner,
                    current.request.utterance_id,
                    SpeechEvent::Error(error),
                );
                state.speaking = None;
                state.paused = false;
            }
        }
    }
}

fn resume_owner(owner: SpeechOwner, state: &mut OwnerSpeech) {
    if !state.paused {
        return;
    }
    state.paused = false;
    if let Some(current) = state.speaking.as_ref()
        && let Some(Ok(voice)) = state.voice.as_mut()
    {
        match voice.resume() {
            Ok(()) => emit(
                &current.sink,
                owner,
                current.request.utterance_id,
                SpeechEvent::Resumed,
            ),
            Err(error) => {
                let _ = voice.cancel();
                emit(
                    &current.sink,
                    owner,
                    current.request.utterance_id,
                    SpeechEvent::Error(error),
                );
                state.speaking = None;
            }
        }
    }
}

fn advance_owner(owner: SpeechOwner, state: &mut OwnerSpeech) {
    if state.paused {
        return;
    }
    if let Some(current) = state.speaking.as_ref()
        && let Some(Ok(voice)) = state.voice.as_ref()
    {
        let event = match voice.is_done() {
            Ok(true) => Some(SpeechEvent::Ended),
            Ok(false) => None,
            Err(error) => Some(SpeechEvent::Error(error)),
        };
        if let Some(event) = event {
            emit(&current.sink, owner, current.request.utterance_id, event);
            state.speaking = None;
        }
    }
    if state.speaking.is_none()
        && let Some(next) = state.queue.pop_front()
    {
        let result = state
            .voice
            .get_or_insert_with(SapiVoice::open)
            .as_mut()
            .map_err(|error| error.clone())
            .and_then(|voice| voice.start(&next.request));
        match result {
            Ok(()) => {
                emit(
                    &next.sink,
                    owner,
                    next.request.utterance_id,
                    SpeechEvent::Started,
                );
                state.speaking = Some(next);
            }
            Err(error) => emit(
                &next.sink,
                owner,
                next.request.utterance_id,
                SpeechEvent::Error(error),
            ),
        }
    }
}

fn cancel_owner(owner: SpeechOwner, state: &mut OwnerSpeech, reset_pause: bool) {
    for pending in state.queue.drain(..) {
        emit(
            &pending.sink,
            owner,
            pending.request.utterance_id,
            SpeechEvent::Cancelled,
        );
    }
    if let Some(current) = state.speaking.take() {
        if let Some(Ok(voice)) = state.voice.as_mut() {
            if state.paused {
                let _ = voice.resume();
            }
            let _ = voice.cancel();
        }
        emit(
            &current.sink,
            owner,
            current.request.utterance_id,
            SpeechEvent::Cancelled,
        );
    }
    if reset_pause {
        state.paused = false;
    }
}

fn emit(sink: &SpeechUpdateSink, owner: SpeechOwner, utterance_id: u64, event: SpeechEvent) {
    let event = match event {
        SpeechEvent::Error(message) => SpeechEvent::Error(bounded_utf8(message, 512)),
        other => other,
    };
    // A renderer that stops draining updates must not block this worker: doing
    // so would delay navigation retirement and allow audio to continue playing.
    let _ = sink.try_send(SpeechUpdate {
        document: owner.document,
        utterance_id,
        event,
    });
}

fn bounded_utf8(mut value: String, maximum_bytes: usize) -> String {
    if value.len() > maximum_bytes {
        let mut end = maximum_bytes;
        while !value.is_char_boundary(end) {
            end -= 1;
        }
        value.truncate(end);
    }
    value
}

fn is_retired(retired: &Mutex<HashMap<TabId, (u64, u64)>>, owner: SpeechOwner) -> bool {
    retired
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .get(&owner.tab)
        .is_some_and(|latest| (owner.session_id, owner.document.get()) <= *latest)
}

#[cfg(test)]
mod tests;
