mod commands;
mod deadlines;
mod document;
mod stream;

use self::document::{IncomingFetchBatch, IncomingPresentation};
use super::diagnostics::{
    RendererExit, RendererExitReason, RendererQueueDepths, RendererTaskTimeout, SharedDiagnostics,
};
use super::queue_depth::QueueDepth;
use super::stream::FetchStreamEvent;
use super::{RendererEvent, RendererState};
use crate::renderer_process::launcher::RendererLaunchOptions;
use crate::renderer_process::windows::{
    exit_code, process_exited, process_sample, terminate_job, wait_for_process,
};
use crate::renderer_protocol::{
    BrowserMessage, DocumentId, DocumentInput, DocumentStart, DocumentState, PresentedViewport,
    ProtocolError, RendererFetchRequest, RendererMessage, RendererPresentation, RestrictionReport,
    StateSnapshotApplied, TestCommand, TransferAssembler,
};
use std::collections::{HashMap, VecDeque};
use std::os::windows::io::OwnedHandle;
use std::sync::{Arc, Mutex, mpsc};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

pub(super) enum BrokerCommand {
    Ping(mpsc::Sender<Result<(), String>>),
    ProbeRestrictions {
        loopback_port: u16,
        reply: mpsc::Sender<Result<RestrictionReport, String>>,
    },
    Test(TestCommand),
    ViewportChanged {
        document: DocumentId,
        viewport: PresentedViewport,
    },
    Input(DocumentInput),
    FullscreenResponse(crate::renderer_protocol::FullscreenResponse),
    Shutdown(mpsc::Sender<Result<RendererExit, String>>),
    Terminate,
    CloseJobForTest(mpsc::Sender<Result<(), String>>),
}

pub(super) enum LifecycleCommand {
    LoadDocument {
        start: Box<DocumentStart>,
        state: DocumentState,
        body: Vec<u8>,
    },
    CancelDocument(DocumentId),
}

pub(super) struct BrokerResources {
    pub(super) process: OwnedHandle,
    pub(super) job: Option<OwnedHandle>,
    pub(super) writer: super::outbound::Sender,
    pub(super) writer_thread: JoinHandle<()>,
    pub(super) incoming: mpsc::Receiver<Result<RendererMessage, ProtocolError>>,
    pub(super) incoming_depth: QueueDepth,
    pub(super) reader_thread: JoinHandle<()>,
    pub(super) commands: mpsc::Receiver<BrokerCommand>,
    pub(super) command_depth: QueueDepth,
    pub(super) acknowledgements: super::acknowledgements::Receiver,
    pub(super) clock: super::clock::Receiver,
    pub(super) state_updates: super::state_updates::Receiver,
    pub(super) lifecycle: mpsc::Receiver<LifecycleCommand>,
    pub(super) fetch_stream: mpsc::Receiver<FetchStreamEvent>,
    pub(super) events: super::events::EventSender,
    pub(super) wake: super::wake::BrokerWake,
    pub(super) shared: Arc<Mutex<SharedDiagnostics>>,
    pub(super) options: RendererLaunchOptions,
}

pub(super) fn run(resources: BrokerResources) {
    let mut broker = Broker::new(resources);
    broker.run();
}

struct Broker {
    resources: Option<BrokerResources>,
    next_ping: u64,
    last_heartbeat: Instant,
    last_sample: Instant,
    pending_pings: HashMap<u64, mpsc::Sender<Result<(), String>>>,
    pending_probe: Option<mpsc::Sender<Result<RestrictionReport, String>>>,
    shutdown_reply: Option<mpsc::Sender<Result<RendererExit, String>>>,
    shutdown_deadline: Option<Instant>,
    shutdown_acknowledged: bool,
    document_load_deadline: Option<(DocumentId, Instant)>,
    exit_reason: Option<RendererExitReason>,
    incoming_fetch: Option<IncomingFetchBatch>,
    incoming_presentation: Option<IncomingPresentation>,
    active_document: Option<DocumentId>,
    retired_document: Option<DocumentId>,
    outgoing_fetch: HashMap<u64, stream::OutgoingFetch>,
    fetch_response_streaming: HashMap<u64, bool>,
    outgoing_state_update: Option<OutgoingStateUpdate>,
}

impl Broker {
    fn new(resources: BrokerResources) -> Self {
        let now = Instant::now();
        Self {
            resources: Some(resources),
            next_ping: 1,
            last_heartbeat: now,
            last_sample: now,
            pending_pings: HashMap::new(),
            pending_probe: None,
            shutdown_reply: None,
            shutdown_deadline: None,
            shutdown_acknowledged: false,
            document_load_deadline: None,
            exit_reason: None,
            incoming_fetch: None,
            incoming_presentation: None,
            active_document: None,
            retired_document: None,
            outgoing_fetch: HashMap::new(),
            fetch_response_streaming: HashMap::new(),
            outgoing_state_update: None,
        }
    }

    fn run(&mut self) {
        self.resources().wake.register_current();
        loop {
            self.process_writer_failure();
            self.process_lifecycle_commands();
            self.process_state_updates();
            self.process_presentation_acknowledgement();
            self.process_commands();
            self.process_document_clock();
            self.process_messages();
            self.process_fetch_stream();
            if self.process_has_exited() {
                self.finish_exit();
                break;
            }
            self.enforce_deadlines();
            self.send_heartbeat();
            self.refresh_metrics();
            self.resources().wake.wait(Duration::from_millis(10));
        }
    }

    fn process_messages(&mut self) {
        for _ in 0..crate::limits::MAX_QUEUED_RENDERER_IPC_MESSAGES {
            let message = match self.resources().incoming.try_recv() {
                Ok(message) => {
                    self.resources().incoming_depth.finish_dequeue();
                    message
                }
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => break,
            };
            match message {
                Ok(RendererMessage::Pong(token)) => {
                    // Token zero is a rate-limited acknowledgement emitted only after the child
                    // completes a command. Real Ping tokens start at one and retain reply routing.
                    {
                        let mut shared = self.shared();
                        shared.last_pong = Instant::now();
                        shared.state = RendererState::Running;
                        shared.active_task = None;
                        shared.active_task_started = None;
                    }
                    if token != 0
                        && let Some(reply) = self.pending_pings.remove(&token)
                    {
                        let _ = reply.send(Ok(()));
                    }
                }
                Ok(RendererMessage::ShutdownComplete) => {
                    if self.shutdown_deadline.is_some() {
                        self.shutdown_acknowledged = true;
                    } else {
                        self.protocol_failure("unsolicited renderer shutdown completion".into());
                    }
                }
                Ok(RendererMessage::Diagnostic(diagnostic)) => {
                    if diagnostic.code == crate::renderer_protocol::RENDERER_DIAGNOSTIC_TASK_STARTED
                    {
                        let mut shared = self.shared();
                        shared.last_pong = Instant::now();
                        shared.state = RendererState::Running;
                        shared.active_task = Some(diagnostic.text);
                        shared.active_task_started = Some(Instant::now());
                        continue;
                    }
                    if diagnostic.code == crate::renderer_protocol::RENDERER_DIAGNOSTIC_TASK_STAGE {
                        self.shared().active_task = Some(diagnostic.text);
                        continue;
                    }
                    if self.exit_reason.is_none() {
                        self.exit_reason = match diagnostic.code {
                            crate::renderer_protocol::RENDERER_DIAGNOSTIC_INTERNAL_ERROR => {
                                Some(RendererExitReason::InternalFailure(diagnostic.text.clone()))
                            }
                            crate::renderer_protocol::RENDERER_DIAGNOSTIC_PROTOCOL_ERROR => {
                                Some(RendererExitReason::ProtocolFailure(diagnostic.text.clone()))
                            }
                            _ => None,
                        };
                    }
                    if let Err(error) = self.emit_event(RendererEvent::Diagnostic {
                        code: diagnostic.code,
                        text: diagnostic.text,
                    }) {
                        self.protocol_failure(error.to_string());
                        break;
                    }
                }
                Ok(RendererMessage::Restrictions(report)) => {
                    if let Some(reply) = self.pending_probe.take() {
                        let _ = reply.send(Ok(report));
                    } else {
                        self.protocol_failure("unsolicited renderer restriction report".into());
                    }
                }
                Ok(
                    message @ (RendererMessage::FetchBatchStart { .. }
                    | RendererMessage::FetchRequestStart { .. }
                    | RendererMessage::FetchRequestChunk(_)
                    | RendererMessage::FetchRequestEnd(_)
                    | RendererMessage::FetchRequestAbort { .. }
                    | RendererMessage::PresentationStart { .. }
                    | RendererMessage::PresentationChunk(_)
                    | RendererMessage::PresentationEnd { .. }
                    | RendererMessage::RuntimeUpdate(_)
                    | RendererMessage::DocumentFailed { .. }
                    | RendererMessage::NavigationRequested { .. }
                    | RendererMessage::PointerCursor(_)
                    | RendererMessage::FullscreenRequest(_)
                    | RendererMessage::CookieMutation(_)
                    | RendererMessage::StorageMutation(_)
                    | RendererMessage::StateSnapshotApplied(_)),
                ) => {
                    if let Err(error) = self.process_document_message(message) {
                        self.protocol_failure(error.to_string());
                    }
                }
                Ok(RendererMessage::Ready { .. }) => {
                    self.protocol_failure("duplicate renderer Ready".into());
                }
                Err(_) if self.shutdown_acknowledged => break,
                Err(ProtocolError::Io(error)) => {
                    if wait_for_process(&self.resources().process, Duration::from_millis(100)) {
                        break;
                    }
                    self.protocol_failure(format!("renderer IPC closed unexpectedly: {error}"));
                }
                Err(error) => self.protocol_failure(error.to_string()),
            }
        }
    }

    fn begin_shutdown(&mut self, reply: Option<mpsc::Sender<Result<RendererExit, String>>>) {
        if let Some(reply) = reply {
            if self.shutdown_reply.is_some() {
                let _ = reply.send(Err("renderer shutdown already pending".into()));
            } else {
                self.shutdown_reply = Some(reply);
            }
        }
        if self.shutdown_deadline.is_none() {
            if self
                .writer()
                .send_browser(&BrowserMessage::Shutdown)
                .is_err()
            {
                self.exit_reason = Some(RendererExitReason::ProtocolFailure(
                    "send renderer shutdown".into(),
                ));
                self.terminate_job(72);
            }
            self.shutdown_deadline =
                Some(Instant::now() + self.resources().options.shutdown_timeout);
        }
    }

    fn refresh_metrics(&mut self) {
        if self.last_sample.elapsed() < Duration::from_secs(1) {
            return;
        }
        self.last_sample = Instant::now();
        self.shared().sample = process_sample(&self.resources().process);
    }

    fn protocol_failure(&mut self, error: String) {
        if self.exit_reason.is_some() {
            return;
        }
        let _ = self
            .writer()
            .send_browser(&BrowserMessage::ProtocolFailure(error.clone()));
        self.exit_reason = Some(RendererExitReason::ProtocolFailure(error));
        self.terminate_job(72);
    }

    fn process_has_exited(&self) -> bool {
        process_exited(&self.resources().process)
    }

    fn finish_exit(&mut self) {
        let resources = self.resources.take().expect("broker resources available");
        wait_for_process(&resources.process, Duration::from_secs(2));
        let code = exit_code(&resources.process).unwrap_or(u32::MAX);
        let default_reason = if self.shutdown_acknowledged && code == 0 {
            RendererExitReason::CleanShutdown
        } else {
            RendererExitReason::Crash
        };
        let requested_reason = resources.shared.lock().unwrap().exit_reason.clone();
        let reason = requested_reason
            .or_else(|| self.exit_reason.clone())
            .unwrap_or(default_reason);
        let uptime = resources.shared.lock().unwrap().started.elapsed();
        let exit = RendererExit {
            process_id: resources.shared.lock().unwrap().process_id,
            code,
            reason,
            uptime,
        };
        {
            let mut shared = resources.shared.lock().unwrap();
            shared.state = RendererState::Exited;
            shared.sample = process_sample(&resources.process);
            shared.exit_reason = Some(exit.reason.clone());
            shared.exit = Some(exit.clone());
        }
        fail_pending(&mut self.pending_pings, &mut self.pending_probe);
        if let Some(reply) = self.shutdown_reply.take() {
            let _ = reply.send(Ok(exit.clone()));
        }
        let _ = resources.events.try_send(RendererEvent::Exited(exit));
        drop(resources.writer);
        drop(resources.job);
        drop(resources.process);
        let _ = resources.writer_thread.join();
        let _ = resources.reader_thread.join();
    }

    fn resources(&self) -> &BrokerResources {
        self.resources.as_ref().expect("broker resources available")
    }

    fn writer(&self) -> &super::outbound::Sender {
        &self.resources().writer
    }

    fn shared(&self) -> std::sync::MutexGuard<'_, SharedDiagnostics> {
        self.resources()
            .shared
            .lock()
            .expect("renderer diagnostics lock poisoned")
    }

    fn terminate_job(&self, code: u32) {
        if let Some(job) = self.resources().job.as_ref() {
            terminate_job(job, code);
        }
    }

    fn process_writer_failure(&mut self) {
        if let Some(error) = self.writer().take_failure()
            && self.exit_reason.is_none()
        {
            self.protocol_failure(format!("renderer IPC writer stopped: {error}"));
        }
    }

    fn emit_event(&self, event: RendererEvent) -> Result<(), ProtocolError> {
        self.resources().events.send(event)
    }

    fn note_renderer_activity(&mut self) {
        let mut shared = self.shared();
        shared.last_pong = Instant::now();
        shared.state = RendererState::Running;
    }
}

struct OutgoingStateUpdate {
    document: DocumentId,
    acknowledgement: StateSnapshotApplied,
    messages: VecDeque<BrowserMessage>,
}

fn fail_pending(
    pings: &mut HashMap<u64, mpsc::Sender<Result<(), String>>>,
    probe: &mut Option<mpsc::Sender<Result<RestrictionReport, String>>>,
) {
    for (_, reply) in pings.drain() {
        let _ = reply.send(Err("renderer exited".into()));
    }
    if let Some(reply) = probe.take() {
        let _ = reply.send(Err("renderer exited".into()));
    }
}
