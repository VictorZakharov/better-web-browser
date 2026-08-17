//! Browser-side renderer session broker and diagnostics.

mod diagnostics;
mod worker;

use super::launcher::{RendererLaunchOptions, launch};
use super::windows::{process_sample, terminate_job, wait_for_process};
use crate::renderer_protocol::{
    BrowserFetchResponse, BrowserMessage, ContainmentReport, DocumentId, DocumentStart,
    FrameReader, FrameWriter, PresentedViewport, ProtocolError, RendererFetchRequest,
    RendererLimits, RendererMessage, RendererPresentation, RendererSessionId, RestrictionReport,
    TestCommand,
};
use std::sync::{Arc, Mutex, mpsc};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use diagnostics::SharedDiagnostics;
pub use diagnostics::{RendererCrashSurface, RendererExit, RendererExitReason};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RendererState {
    Running,
    Unresponsive,
    Exited,
}

#[derive(Clone, Debug)]
pub struct RendererSnapshot {
    pub process_id: u32,
    pub session_id: u64,
    pub state: RendererState,
    pub working_set: usize,
    pub private_memory: usize,
    pub peak_working_set: usize,
    pub cpu_ticks: u64,
    pub handle_count: u32,
    pub uptime: Duration,
    pub last_pong_age: Duration,
    pub exit_reason: Option<RendererExitReason>,
}

#[derive(Clone, Debug)]
pub enum RendererEvent {
    Diagnostic {
        code: u16,
        text: String,
    },
    FetchBatch(Vec<RendererFetchRequest>),
    Presentation(Box<RendererPresentation>),
    TimeAdvanced {
        document: DocumentId,
        next_timer_micros: Option<u64>,
    },
    DocumentFailed {
        document: DocumentId,
        detail: String,
    },
    NavigationRequested {
        document: DocumentId,
        url: String,
    },
    Unresponsive,
    Exited(RendererExit),
}

pub struct RendererSession {
    commands: mpsc::Sender<worker::BrokerCommand>,
    events: mpsc::Receiver<RendererEvent>,
    shared: Arc<Mutex<SharedDiagnostics>>,
    worker: Option<JoinHandle<()>>,
    shutdown_timeout: Duration,
}

impl RendererSession {
    pub fn launch(options: RendererLaunchOptions) -> Result<Self, String> {
        let launched = launch(&options)?;
        let session = launched.session;
        let nonce = launched.nonce;
        let process_id = launched.process_id;
        let mut writer = FrameWriter::new(launched.browser_output, session);
        let (incoming, reader_thread) = spawn_reader(launched.browser_input, session)?;
        let limits = RendererLimits {
            heartbeat_millis: duration_millis(options.heartbeat_interval),
            ..RendererLimits::default()
        };
        if let Err(error) = writer.send_browser(&BrowserMessage::Hello { nonce, limits }) {
            terminate_startup(&launched.process, &launched.job, reader_thread);
            return Err(format!("send renderer hello: {error}"));
        }
        let ready = match incoming.recv_timeout(options.startup_timeout) {
            Ok(ready) => ready,
            Err(error) => {
                terminate_startup(&launched.process, &launched.job, reader_thread);
                return Err(format!(
                    "renderer startup handshake timed out or disconnected: {error}"
                ));
            }
        };
        let ready = match ready {
            Ok(ready) => ready,
            Err(error) => {
                terminate_startup(&launched.process, &launched.job, reader_thread);
                return Err(format!("renderer startup protocol failed: {error}"));
            }
        };
        let RendererMessage::Ready {
            nonce: echoed_nonce,
            containment,
        } = ready
        else {
            terminate_startup(&launched.process, &launched.job, reader_thread);
            return Err("renderer did not send Ready during startup".into());
        };
        if let Err(error) = validate_ready(nonce, echoed_nonce, containment) {
            terminate_startup(&launched.process, &launched.job, reader_thread);
            return Err(error);
        }

        let sample = process_sample(&launched.process);
        let now = Instant::now();
        let shared = Arc::new(Mutex::new(SharedDiagnostics {
            process_id,
            session,
            state: RendererState::Running,
            sample,
            started: now,
            last_pong: now,
            exit_reason: None,
        }));
        let (commands_tx, commands_rx) = mpsc::channel();
        let (events_tx, events_rx) = mpsc::channel();
        let worker_shared = Arc::clone(&shared);
        let worker_options = options.clone();
        let handle = std::thread::Builder::new()
            .name("breeze-renderer-broker".into())
            .spawn(move || {
                worker::run(worker::BrokerResources {
                    process: launched.process,
                    job: Some(launched.job),
                    writer,
                    incoming,
                    reader_thread,
                    commands: commands_rx,
                    events: events_tx,
                    shared: worker_shared,
                    options: worker_options,
                });
            })
            .map_err(|error| format!("start renderer broker thread: {error}"))?;
        Ok(Self {
            commands: commands_tx,
            events: events_rx,
            shared,
            worker: Some(handle),
            shutdown_timeout: options.shutdown_timeout,
        })
    }

    pub fn snapshot(&self) -> RendererSnapshot {
        self.shared
            .lock()
            .expect("renderer diagnostics lock poisoned")
            .snapshot()
    }

    pub fn ping(&self, timeout: Duration) -> Result<(), String> {
        let (reply, response) = mpsc::channel();
        self.commands
            .send(worker::BrokerCommand::Ping(reply))
            .map_err(|_| "renderer broker has exited".to_string())?;
        response
            .recv_timeout(timeout)
            .map_err(|_| "renderer ping timed out".to_string())?
    }

    pub fn probe_restrictions(
        &self,
        loopback_port: u16,
        timeout: Duration,
    ) -> Result<RestrictionReport, String> {
        let (reply, response) = mpsc::channel();
        self.commands
            .send(worker::BrokerCommand::ProbeRestrictions {
                loopback_port,
                reply,
            })
            .map_err(|_| "renderer broker has exited".to_string())?;
        response
            .recv_timeout(timeout)
            .map_err(|_| "renderer restriction probe timed out".to_string())?
    }

    pub fn send_test_command(&self, command: TestCommand) -> Result<(), String> {
        self.commands
            .send(worker::BrokerCommand::Test(command))
            .map_err(|_| "renderer broker has exited".to_string())
    }

    pub fn load_document(&self, start: DocumentStart, body: Vec<u8>) -> Result<(), String> {
        self.commands
            .send(worker::BrokerCommand::LoadDocument { start, body })
            .map_err(|_| "renderer broker has exited".to_string())
    }

    pub fn complete_fetch_batch(&self, responses: Vec<BrowserFetchResponse>) -> Result<(), String> {
        self.commands
            .send(worker::BrokerCommand::CompleteFetchBatch(responses))
            .map_err(|_| "renderer broker has exited".to_string())
    }

    pub fn advance_time(
        &self,
        document: DocumentId,
        elapsed: Duration,
        max_callbacks: u32,
    ) -> Result<(), String> {
        self.commands
            .send(worker::BrokerCommand::AdvanceTime {
                document,
                elapsed,
                max_callbacks,
            })
            .map_err(|_| "renderer broker has exited".to_string())
    }

    pub fn update_viewport(
        &self,
        document: DocumentId,
        viewport: PresentedViewport,
    ) -> Result<(), String> {
        self.commands
            .send(worker::BrokerCommand::ViewportChanged { document, viewport })
            .map_err(|_| "renderer broker has exited".to_string())
    }

    pub fn cancel_document(&self, document: DocumentId) -> Result<(), String> {
        self.commands
            .send(worker::BrokerCommand::CancelDocument(document))
            .map_err(|_| "renderer broker has exited".to_string())
    }

    pub fn wait_for_event(&self, timeout: Duration) -> Result<RendererEvent, String> {
        self.events
            .recv_timeout(timeout)
            .map_err(|_| "renderer event timed out".to_string())
    }

    pub fn try_event(&self) -> Result<Option<RendererEvent>, String> {
        match self.events.try_recv() {
            Ok(event) => Ok(Some(event)),
            Err(mpsc::TryRecvError::Empty) => Ok(None),
            Err(mpsc::TryRecvError::Disconnected) => Err("renderer broker has exited".into()),
        }
    }

    pub fn wait_for_exit(&self, timeout: Duration) -> Result<RendererExit, String> {
        let deadline = Instant::now() + timeout;
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err("renderer exit timed out".into());
            }
            if let RendererEvent::Exited(exit) = self.wait_for_event(remaining)? {
                return Ok(exit);
            }
        }
    }

    pub fn terminate(&self) -> Result<(), String> {
        self.commands
            .send(worker::BrokerCommand::Terminate)
            .map_err(|_| "renderer broker has exited".to_string())
    }

    /// Requests immediate renderer termination without waiting on the caller's thread.
    ///
    /// Browser UI recovery paths use this instead of `Drop`, whose graceful shutdown wait is
    /// intentionally bounded but can still take several seconds for an unresponsive renderer.
    pub fn terminate_in_background(mut self) {
        let _ = self.commands.send(worker::BrokerCommand::Terminate);
        let Some(worker) = self.worker.take() else {
            return;
        };
        let _ = std::thread::Builder::new()
            .name("breeze-renderer-reaper".into())
            .spawn(move || {
                let _ = worker.join();
            });
    }

    #[doc(hidden)]
    pub fn close_job_for_test(&self) -> Result<(), String> {
        let (reply, response) = mpsc::channel();
        self.commands
            .send(worker::BrokerCommand::CloseJobForTest(reply))
            .map_err(|_| "renderer broker has exited".to_string())?;
        response
            .recv_timeout(Duration::from_secs(1))
            .map_err(|_| "renderer Job close timed out".to_string())?
    }

    pub fn shutdown(&mut self) -> Result<RendererExit, String> {
        let (reply, response) = mpsc::channel();
        self.commands
            .send(worker::BrokerCommand::Shutdown(reply))
            .map_err(|_| "renderer broker has exited".to_string())?;
        let result = response
            .recv_timeout(self.shutdown_timeout + Duration::from_secs(1))
            .map_err(|_| "renderer shutdown timed out".to_string())?;
        self.join_worker();
        result
    }

    fn join_worker(&mut self) {
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

impl Drop for RendererSession {
    fn drop(&mut self) {
        if self.worker.is_none() {
            return;
        }
        let (reply, response) = mpsc::channel();
        let _ = self.commands.send(worker::BrokerCommand::Shutdown(reply));
        if response
            .recv_timeout(self.shutdown_timeout + Duration::from_secs(1))
            .is_err()
        {
            let _ = self.commands.send(worker::BrokerCommand::Terminate);
        }
        self.join_worker();
    }
}

type RendererIncoming = mpsc::Receiver<Result<RendererMessage, ProtocolError>>;

fn spawn_reader(
    input: std::fs::File,
    session: RendererSessionId,
) -> Result<(RendererIncoming, JoinHandle<()>), String> {
    let (sender, receiver) = mpsc::channel();
    let handle = std::thread::Builder::new()
        .name("breeze-renderer-ipc-read".into())
        .spawn(move || {
            let mut reader = FrameReader::new(input, session);
            loop {
                let message = reader.read_renderer();
                let failed = message.is_err();
                if sender.send(message).is_err() || failed {
                    break;
                }
            }
        })
        .map_err(|error| format!("start renderer IPC reader: {error}"))?;
    Ok((receiver, handle))
}

fn validate_ready(
    expected: crate::renderer_protocol::Nonce,
    actual: crate::renderer_protocol::Nonce,
    containment: ContainmentReport,
) -> Result<(), String> {
    if expected != actual {
        return Err("renderer returned a stale bootstrap nonce".into());
    }
    if !containment.app_container {
        return Err("renderer did not start in an AppContainer".into());
    }
    if !containment.no_console_window {
        return Err("renderer unexpectedly owns a console window".into());
    }
    if !containment.minimal_environment {
        return Err("renderer inherited an unsafe process environment".into());
    }
    Ok(())
}

fn terminate_startup(
    process: &std::os::windows::io::OwnedHandle,
    job: &std::os::windows::io::OwnedHandle,
    reader: JoinHandle<()>,
) {
    terminate_job(job, 71);
    wait_for_process(process, Duration::from_secs(2));
    let _ = reader.join();
}

fn duration_millis(duration: Duration) -> u32 {
    duration.as_millis().clamp(1, u32::MAX as u128) as u32
}
