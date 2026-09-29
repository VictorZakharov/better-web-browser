//! Browser-side renderer session broker and diagnostics.

mod acknowledgements;
mod capture_stream;
mod clock;
mod control;
mod diagnostics;
mod events;
mod flow;
mod navigation;
mod outbound;
mod queue_depth;
mod session;
mod state_updates;
pub(super) mod stream;
#[cfg(test)]
mod tests;
mod viewport;
mod wake;
mod worker;
use super::launcher::{RendererLaunchOptions, launch};
use super::windows::{process_sample, terminate_job, terminate_job_checked, wait_for_process};
use crate::renderer_protocol::{
    BrowserMessage, BrowsingContextId, ContainmentReport, CookieMutation, DocumentId, FrameReader,
    FrameWriter, NavigationCause, NavigationDisposition, PointerCursorResult, PolicyMutation,
    ProtocolError, RendererFetchRequest, RendererLimits, RendererMessage, RendererPresentation,
    RendererRuntimeUpdate, RendererSessionId, RestrictionReport, StorageMutationRequest,
    TestCommand,
};
use diagnostics::SharedDiagnostics;
pub use diagnostics::{
    RendererCrashSurface, RendererExit, RendererExitReason, RendererQueueDepths,
};
pub type RendererTaskTimeout = diagnostics::RendererTaskTimeout;

impl RendererExitReason {
    pub fn task_timeout(&self) -> Option<&RendererTaskTimeout> {
        match self {
            Self::TaskBudgetExceeded(timeout) => Some(timeout),
            _ => None,
        }
    }
}
pub use capture_stream::MediaCaptureSink;
pub use navigation::NavigationBody;
use queue_depth::QueueDepth;
use std::sync::{Arc, Mutex, mpsc};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};
pub use stream::{
    ClipboardUpdateSink, DatabaseEventSink, FetchResponseSink, FilePickerUpdateSink,
    GeolocationUpdateSink, MediaDeviceUpdateSink, NotificationUpdateSink, PermissionUpdateSink,
    SensorUpdateSink, SpeechUpdateSink, WebSocketEventSink,
};
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
    pub context_id: u64,
    pub state: RendererState,
    pub working_set: usize,
    pub private_memory: usize,
    pub peak_working_set: usize,
    pub cpu_ticks: u64,
    pub handle_count: u32,
    pub uptime: Duration,
    pub last_pong_age: Duration,
    pub active_task: Option<String>,
    pub active_task_elapsed: Option<Duration>,
    pub queues: RendererQueueDepths,
    pub pending_state_updates: usize,
    pub submitted_state_updates: u64,
    pub coalesced_state_updates: u64,
    pub exit_reason: Option<RendererExitReason>,
    pub exit: Option<RendererExit>,
}

#[derive(Clone, Debug)]
pub enum RendererEvent {
    Diagnostic {
        code: u16,
        text: String,
    },
    FetchBatch {
        document: DocumentId,
        requests: Vec<RendererFetchRequest>,
    },
    FetchAbort {
        document: DocumentId,
        request_id: u64,
    },
    Presentation(Box<RendererPresentation>),
    VideoFrame(Box<crate::renderer_protocol::VideoFrameUpdate>),
    RuntimeUpdate(Box<RendererRuntimeUpdate>),
    DocumentFailed {
        document: DocumentId,
        detail: String,
    },
    NavigationRequested {
        document: DocumentId,
        url: String,
        disposition: NavigationDisposition,
        cause: NavigationCause,
    },
    PointerCursor(PointerCursorResult),
    TextSelectionUpdate(crate::renderer_protocol::TextSelectionUpdate),
    FullscreenRequested(crate::renderer_protocol::FullscreenRequest),
    PointerLockRequested(crate::renderer_protocol::PointerLockRequest),
    WakeLockRequested(crate::renderer_protocol::WakeLockRequest),
    CookieMutation(CookieMutation),
    PolicyMutation(PolicyMutation),
    StorageMutation(StorageMutationRequest),
    BroadcastCommand(crate::renderer_protocol::BroadcastCommand),
    WebSocketCommand(crate::renderer_protocol::WebSocketCommand),
    DatabaseCommand(crate::renderer_protocol::DatabaseCommand),
    SpeechRequest(crate::renderer_protocol::SpeechRequest),
    NotificationRequest(crate::renderer_protocol::NotificationRequest),
    ProtocolHandlerRequest(crate::renderer_protocol::ProtocolHandlerRequest),
    PermissionRequest(crate::renderer_protocol::PermissionRequest),
    GeolocationRequest(crate::renderer_protocol::GeolocationRequest),
    MediaDeviceRequest(crate::renderer_protocol::MediaDeviceRequest),
    MediaCaptureRequest(crate::renderer_protocol::MediaCaptureRequest),
    SensorRequest(crate::renderer_protocol::SensorRequest),
    ClipboardRequest(crate::renderer_protocol::ClipboardRequest),
    FilePickerRequest(crate::renderer_protocol::FilePickerRequest),
    Unresponsive,
    Exited(RendererExit),
}

pub struct RendererSession {
    commands: mpsc::SyncSender<worker::BrokerCommand>,
    command_depth: QueueDepth,
    acknowledgements: acknowledgements::Sender,
    clock: clock::Sender,
    viewport: viewport::Sender,
    state_updates: state_updates::Sender,
    lifecycle: mpsc::Sender<worker::LifecycleCommand>,
    fetch_stream: mpsc::SyncSender<stream::FetchStreamEvent>,
    database_events: mpsc::SyncSender<crate::renderer_protocol::DatabaseEvent>,
    database_queued_bytes: Arc<std::sync::atomic::AtomicUsize>,
    database_overflow: Arc<std::sync::atomic::AtomicBool>,
    notification_updates: mpsc::SyncSender<crate::renderer_protocol::NotificationUpdate>,
    notification_overflow: Arc<std::sync::atomic::AtomicBool>,
    permission_updates: mpsc::SyncSender<crate::renderer_protocol::PermissionUpdate>,
    permission_overflow: Arc<std::sync::atomic::AtomicBool>,
    geolocation_updates: mpsc::SyncSender<crate::renderer_protocol::GeolocationUpdate>,
    geolocation_overflow: Arc<std::sync::atomic::AtomicBool>,
    media_device_updates: mpsc::SyncSender<crate::renderer_protocol::MediaDeviceUpdate>,
    media_device_overflow: Arc<std::sync::atomic::AtomicBool>,
    media_capture_updates: mpsc::SyncSender<crate::renderer_protocol::MediaCaptureUpdate>,
    media_capture_overflow: Arc<std::sync::atomic::AtomicBool>,
    media_capture_frames: Arc<Mutex<capture_stream::FrameMailbox>>,
    sensor_updates: mpsc::SyncSender<stream::QueuedSensorUpdate>,
    sensor_overflow: Arc<std::sync::atomic::AtomicBool>,
    clipboard_updates: mpsc::SyncSender<crate::renderer_protocol::ClipboardUpdate>,
    clipboard_overflow: Arc<std::sync::atomic::AtomicBool>,
    file_picker_updates: mpsc::SyncSender<crate::renderer_protocol::FilePickerUpdate>,
    file_picker_overflow: Arc<std::sync::atomic::AtomicBool>,
    fetch_flow: Arc<flow::FetchFlow>,
    events: events::EventReceiver,
    incoming_depth: QueueDepth,
    outbound_diagnostics: outbound::Diagnostics,
    wake: wake::BrokerWake,
    shared: Arc<Mutex<SharedDiagnostics>>,
    termination_job: std::os::windows::io::OwnedHandle,
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
        let wake = wake::BrokerWake::default();
        let (incoming, incoming_depth, reader_thread) =
            spawn_reader(launched.browser_input, session, wake.clone())?;
        let limits = RendererLimits {
            heartbeat_millis: duration_millis(options.heartbeat_interval),
            ..RendererLimits::default()
        };
        let context = options.browsing_context;
        if let Err(error) = writer.send_browser(&BrowserMessage::Hello {
            nonce,
            context,
            limits,
        }) {
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
            context: echoed_context,
            containment,
        } = ready
        else {
            terminate_startup(&launched.process, &launched.job, reader_thread);
            return Err("renderer did not send Ready during startup".into());
        };
        if let Err(error) =
            validate_ready(nonce, echoed_nonce, context, echoed_context, containment)
        {
            terminate_startup(&launched.process, &launched.job, reader_thread);
            return Err(error);
        }

        let termination_job = match launched.job.try_clone() {
            Ok(job) => job,
            Err(error) => {
                terminate_startup(&launched.process, &launched.job, reader_thread);
                return Err(format!("duplicate renderer Job handle: {error}"));
            }
        };
        let sample = process_sample(&launched.process);
        let now = Instant::now();
        let shared = Arc::new(Mutex::new(SharedDiagnostics {
            process_id,
            session,
            context,
            state: RendererState::Running,
            sample,
            started: now,
            last_pong: now,
            active_task: None,
            active_task_started: None,
            exit_reason: None,
            exit: None,
        }));
        let (outbound, outbound_diagnostics, writer_thread) =
            match outbound::spawn(writer, session, wake.clone()) {
                Ok(writer) => writer,
                Err(error) => {
                    terminate_startup(&launched.process, &launched.job, reader_thread);
                    return Err(error);
                }
            };
        let command_depth = QueueDepth::default();
        let (commands_tx, commands_rx) =
            mpsc::sync_channel(crate::limits::MAX_QUEUED_BROWSER_COMMANDS);
        // Browser-owned progress must not compete with bounded page-generated commands.
        let (acknowledgements_tx, acknowledgements_rx) = acknowledgements::bounded();
        let (clock_tx, clock_rx) = clock::bounded();
        let (viewport_tx, viewport_rx) = viewport::bounded();
        let (state_updates_tx, state_updates_rx) = state_updates::bounded();
        // Browser state serializes replacement to one lossless cancel plus one pending page.
        let (lifecycle_tx, lifecycle_rx) = mpsc::channel();
        let (fetch_stream_tx, fetch_stream_rx) =
            mpsc::sync_channel(crate::limits::MAX_QUEUED_FETCH_STREAM_CHUNKS);
        // Required database replies cannot wait behind Fetch body chunks.
        let (database_events_tx, database_events_rx) =
            mpsc::sync_channel(crate::limits::MAX_QUEUED_FETCH_STREAM_CHUNKS);
        let database_queued_bytes = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let database_overflow = Arc::new(std::sync::atomic::AtomicBool::new(false));
        // Permission decisions and terminal Notification events settle renderer state.
        // They cannot compete with bulk Fetch chunks in the lossy UI-thread lane.
        let (notification_updates_tx, notification_updates_rx) = mpsc::sync_channel(256);
        let notification_overflow = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let (permission_updates_tx, permission_updates_rx) = mpsc::sync_channel(256);
        let permission_overflow = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let (geolocation_updates_tx, geolocation_updates_rx) = mpsc::sync_channel(256);
        let geolocation_overflow = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let (media_device_updates_tx, media_device_updates_rx) = mpsc::sync_channel(256);
        let media_device_overflow = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let (media_capture_updates_tx, media_capture_updates_rx) = mpsc::sync_channel(64);
        let media_capture_overflow = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let media_capture_frames = Arc::new(Mutex::new(capture_stream::FrameMailbox::default()));
        let (sensor_updates_tx, sensor_updates_rx) = mpsc::sync_channel(256);
        let sensor_overflow = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let (clipboard_updates_tx, clipboard_updates_rx) = mpsc::sync_channel(256);
        let clipboard_overflow = Arc::new(std::sync::atomic::AtomicBool::new(false));
        // Eight partial files can need 71 chunks at the 4 MiB/64 KiB limits,
        // plus Start and End. Keep one whole selection bounded in this lane.
        let (file_picker_updates_tx, file_picker_updates_rx) = mpsc::sync_channel(80);
        let file_picker_overflow = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let fetch_flow = Arc::new(flow::FetchFlow::default());
        let worker_fetch_flow = Arc::clone(&fetch_flow);
        let (events_tx, events_rx) = events::bounded();
        let worker_shared = Arc::clone(&shared);
        let worker_options = options.clone();
        let worker_wake = wake.clone();
        let worker_command_depth = command_depth.clone();
        let worker_incoming_depth = incoming_depth.clone();
        let worker_notification_overflow = Arc::clone(&notification_overflow);
        let worker_permission_overflow = Arc::clone(&permission_overflow);
        let worker_geolocation_overflow = Arc::clone(&geolocation_overflow);
        let worker_media_device_overflow = Arc::clone(&media_device_overflow);
        let worker_media_capture_overflow = Arc::clone(&media_capture_overflow);
        let worker_media_capture_frames = Arc::clone(&media_capture_frames);
        let worker_sensor_overflow = Arc::clone(&sensor_overflow);
        let worker_clipboard_overflow = Arc::clone(&clipboard_overflow);
        let worker_file_picker_overflow = Arc::clone(&file_picker_overflow);
        let worker_database_overflow = Arc::clone(&database_overflow);
        let worker_database_queued_bytes = Arc::clone(&database_queued_bytes);
        let handle = std::thread::Builder::new()
            .name("breeze-renderer-broker".into())
            .spawn(move || {
                worker::run(worker::BrokerResources {
                    process: launched.process,
                    job: Some(launched.job),
                    media: launched.media,
                    writer: outbound,
                    writer_thread,
                    incoming,
                    incoming_depth: worker_incoming_depth,
                    reader_thread,
                    commands: commands_rx,
                    command_depth: worker_command_depth,
                    acknowledgements: acknowledgements_rx,
                    clock: clock_rx,
                    viewport: viewport_rx,
                    state_updates: state_updates_rx,
                    lifecycle: lifecycle_rx,
                    fetch_stream: fetch_stream_rx,
                    database_events: database_events_rx,
                    database_queued_bytes: worker_database_queued_bytes,
                    database_overflow: worker_database_overflow,
                    notification_updates: notification_updates_rx,
                    notification_overflow: worker_notification_overflow,
                    permission_updates: permission_updates_rx,
                    permission_overflow: worker_permission_overflow,
                    geolocation_updates: geolocation_updates_rx,
                    geolocation_overflow: worker_geolocation_overflow,
                    media_device_updates: media_device_updates_rx,
                    media_device_overflow: worker_media_device_overflow,
                    media_capture_updates: media_capture_updates_rx,
                    media_capture_overflow: worker_media_capture_overflow,
                    media_capture_frames: worker_media_capture_frames,
                    sensor_updates: sensor_updates_rx,
                    sensor_overflow: worker_sensor_overflow,
                    clipboard_updates: clipboard_updates_rx,
                    clipboard_overflow: worker_clipboard_overflow,
                    file_picker_updates: file_picker_updates_rx,
                    file_picker_overflow: worker_file_picker_overflow,
                    fetch_flow: worker_fetch_flow,
                    events: events_tx,
                    wake: worker_wake,
                    shared: worker_shared,
                    options: worker_options,
                });
            })
            .map_err(|error| format!("start renderer broker thread: {error}"))?;
        Ok(Self {
            commands: commands_tx,
            command_depth,
            acknowledgements: acknowledgements_tx,
            clock: clock_tx,
            viewport: viewport_tx,
            state_updates: state_updates_tx,
            lifecycle: lifecycle_tx,
            fetch_stream: fetch_stream_tx,
            database_events: database_events_tx,
            database_queued_bytes,
            database_overflow,
            notification_updates: notification_updates_tx,
            notification_overflow,
            permission_updates: permission_updates_tx,
            permission_overflow,
            geolocation_updates: geolocation_updates_tx,
            geolocation_overflow,
            media_device_updates: media_device_updates_tx,
            media_device_overflow,
            media_capture_updates: media_capture_updates_tx,
            media_capture_overflow,
            media_capture_frames,
            sensor_updates: sensor_updates_tx,
            sensor_overflow,
            clipboard_updates: clipboard_updates_tx,
            clipboard_overflow,
            file_picker_updates: file_picker_updates_tx,
            file_picker_overflow,
            fetch_flow,
            events: events_rx,
            incoming_depth,
            outbound_diagnostics,
            wake,
            shared,
            termination_job,
            worker: Some(handle),
            shutdown_timeout: options.shutdown_timeout,
        })
    }
}

type RendererIncoming = mpsc::Receiver<Result<RendererMessage, ProtocolError>>;

fn spawn_reader(
    input: std::fs::File,
    session: RendererSessionId,
    wake: wake::BrokerWake,
) -> Result<(RendererIncoming, QueueDepth, JoinHandle<()>), String> {
    let (sender, receiver) = mpsc::sync_channel(crate::limits::MAX_QUEUED_RENDERER_IPC_MESSAGES);
    let depth = QueueDepth::default();
    let reader_depth = depth.clone();
    let handle = std::thread::Builder::new()
        .name("breeze-renderer-ipc-read".into())
        .spawn(move || {
            let mut reader = FrameReader::new(input, session);
            loop {
                let message = reader.read_renderer();
                let failed = message.is_err();
                reader_depth.begin_enqueue();
                wake.notify();
                if sender.send(message).is_err() {
                    reader_depth.finish_dequeue();
                    break;
                }
                wake.notify();
                if failed {
                    break;
                }
            }
        })
        .map_err(|error| format!("start renderer IPC reader: {error}"))?;
    Ok((receiver, depth, handle))
}

fn validate_ready(
    expected: crate::renderer_protocol::Nonce,
    actual: crate::renderer_protocol::Nonce,
    expected_context: BrowsingContextId,
    actual_context: BrowsingContextId,
    containment: ContainmentReport,
) -> Result<(), String> {
    if expected != actual {
        return Err("renderer returned a stale bootstrap nonce".into());
    }
    if expected_context != actual_context {
        return Err("renderer returned a stale browsing context".into());
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
