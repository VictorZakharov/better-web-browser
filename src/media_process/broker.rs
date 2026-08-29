mod testing;

use super::launcher::{MediaLaunchOptions, launch};
use crate::media_protocol::{
    BrowserMediaMessage, ContainmentReport, MediaCapabilityReport, MediaFrameReader,
    MediaFrameWriter, MediaLimits, MediaProtocolError, WorkerMediaMessage,
};
use crate::renderer_process::windows::{
    exit_code, process_sample, terminate_job, wait_for_process,
};
use std::fs::File;
use std::os::windows::io::OwnedHandle;
use std::sync::mpsc::{self, Receiver};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

const MEDIA_EXIT_STARTUP: u32 = 0x4d01;
const MEDIA_EXIT_PROTOCOL: u32 = 0x4d02;
const MEDIA_EXIT_TIMEOUT: u32 = 0x4d03;
const MEDIA_EXIT_DROP: u32 = 0x4d04;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MediaWorkerState {
    Running,
    Exited,
}

#[derive(Clone, Debug)]
pub struct DecodedMediaFrame {
    pub metadata: crate::media_protocol::MediaVideoFrameMetadata,
    pub nv12: Vec<u8>,
    pub bgra: Vec<u8>,
}

#[derive(Clone, Debug)]
pub struct OwnedMediaDecode {
    pub report: crate::media_protocol::MediaDecodeReport,
    pub frame: DecodedMediaFrame,
}

#[derive(Clone, Debug)]
pub struct MediaWorkerSnapshot {
    pub process_id: u32,
    pub session_id: u64,
    pub state: MediaWorkerState,
    pub containment: ContainmentReport,
    pub working_set: usize,
    pub private_memory: usize,
    pub peak_working_set: usize,
    pub cpu_ticks: u64,
    pub handle_count: u32,
    pub uptime: Duration,
    pub last_progress_age: Duration,
    pub limits: MediaLimits,
    pub capability: Option<MediaCapabilityReport>,
    pub exit_code: Option<u32>,
    pub exit_reason: Option<String>,
}

type Incoming = Receiver<Result<WorkerMediaMessage, MediaProtocolError>>;

/// Owns one restricted media worker. The foundation is intentionally synchronous: the browser
/// must not enqueue an unbounded stream of media control work while a worker is unavailable.
pub struct MediaSession {
    writer: MediaFrameWriter<File>,
    data_output: File,
    frame_input: File,
    incoming: Incoming,
    reader: Option<JoinHandle<()>>,
    process: OwnedHandle,
    job: OwnedHandle,
    process_id: u32,
    session_id: u64,
    nonce: crate::media_protocol::Nonce,
    limits: MediaLimits,
    containment: ContainmentReport,
    command_timeout: Duration,
    shutdown_timeout: Duration,
    started: Instant,
    last_progress: Instant,
    next_request: u64,
    next_source: u64,
    next_frame: u64,
    state: MediaWorkerState,
    capability: Option<MediaCapabilityReport>,
    exit_code: Option<u32>,
    exit_reason: Option<String>,
    test_mode: bool,
}

impl MediaSession {
    pub fn launch(options: MediaLaunchOptions) -> Result<Self, String> {
        let launched = launch(&options)?;
        let session = launched.session;
        let nonce = launched.nonce;
        let mut writer = MediaFrameWriter::new(launched.browser_output, session);
        let (incoming, reader) = spawn_reader(launched.browser_input, session)?;
        let limits = MediaLimits::default();
        if let Err(error) = writer.send_browser(&BrowserMediaMessage::Hello { nonce, limits }) {
            terminate_startup(&launched.process, &launched.job, reader);
            return Err(format!("send media hello: {error}"));
        }
        let ready = match incoming.recv_timeout(options.startup_timeout) {
            Ok(Ok(message)) => message,
            Ok(Err(error)) => {
                terminate_startup(&launched.process, &launched.job, reader);
                return Err(format!("media startup protocol failed: {error}"));
            }
            Err(error) => {
                terminate_startup(&launched.process, &launched.job, reader);
                return Err(format!(
                    "media startup handshake timed out or disconnected: {error}"
                ));
            }
        };
        let WorkerMediaMessage::Ready {
            nonce: echoed_nonce,
            containment,
        } = ready
        else {
            terminate_startup(&launched.process, &launched.job, reader);
            return Err("media worker did not send Ready during startup".into());
        };
        if let Err(error) = validate_ready(nonce, echoed_nonce, containment) {
            terminate_startup(&launched.process, &launched.job, reader);
            return Err(error);
        }

        let now = Instant::now();
        Ok(Self {
            writer,
            data_output: launched.browser_data_output,
            frame_input: launched.browser_frame_input,
            incoming,
            reader: Some(reader),
            process: launched.process,
            job: launched.job,
            process_id: launched.process_id,
            session_id: session.get(),
            nonce,
            limits,
            containment,
            command_timeout: options.command_timeout,
            shutdown_timeout: options.shutdown_timeout,
            started: now,
            last_progress: now,
            next_request: 1,
            next_source: 1,
            next_frame: 1,
            state: MediaWorkerState::Running,
            capability: None,
            exit_code: None,
            exit_reason: None,
            test_mode: options.test_mode,
        })
    }

    pub fn probe(&mut self) -> Result<MediaCapabilityReport, String> {
        let request_id = self.next_request;
        self.next_request = request_id
            .checked_add(1)
            .ok_or_else(|| "media request identity exhausted".to_string())?;
        self.send(BrowserMediaMessage::Probe { request_id }, "probe")?;
        let response = self.receive("probe", self.command_timeout)?;
        let WorkerMediaMessage::Capability {
            request_id: actual,
            report,
        } = response
        else {
            return self.protocol_failure("media worker returned the wrong probe response");
        };
        if actual != request_id {
            return self.protocol_failure("media worker returned a stale probe response");
        }
        if let Err(error) = report.validate(self.limits) {
            return self.protocol_failure(&format!("invalid media capability report: {error}"));
        }
        self.capability = Some(report);
        Ok(report)
    }

    pub fn ping(&mut self, token: u64) -> Result<(), String> {
        self.send(BrowserMediaMessage::Ping(token), "ping")?;
        match self.receive("ping", self.command_timeout)? {
            WorkerMediaMessage::Pong(actual) if actual == token => Ok(()),
            _ => self.protocol_failure("media worker returned the wrong ping response"),
        }
    }

    pub fn snapshot(&self) -> MediaWorkerSnapshot {
        let sample = process_sample(&self.process);
        MediaWorkerSnapshot {
            process_id: self.process_id,
            session_id: self.session_id,
            state: self.state,
            containment: self.containment,
            working_set: sample.working_set,
            private_memory: sample.private_memory,
            peak_working_set: sample.peak_working_set,
            cpu_ticks: sample.cpu_ticks,
            handle_count: sample.handle_count,
            uptime: self.started.elapsed(),
            last_progress_age: self.last_progress.elapsed(),
            limits: self.limits,
            capability: self.capability,
            exit_code: self.exit_code,
            exit_reason: self.exit_reason.clone(),
        }
    }

    pub fn shutdown(&mut self) -> Result<(), String> {
        if self.state == MediaWorkerState::Exited {
            return Ok(());
        }
        self.send(BrowserMediaMessage::Shutdown, "shutdown")?;
        match self.receive("shutdown", self.shutdown_timeout)? {
            WorkerMediaMessage::ShutdownComplete => {}
            _ => return self.protocol_failure("media worker returned the wrong shutdown response"),
        }
        if !wait_for_process(&self.process, self.shutdown_timeout) {
            self.mark_exited(
                "media worker did not exit after shutdown".into(),
                MEDIA_EXIT_TIMEOUT,
            );
            return Err(self.exit_reason.clone().unwrap_or_default());
        }
        self.finish_exit("media worker shut down cleanly".into());
        Ok(())
    }

    fn send(&mut self, message: BrowserMediaMessage, operation: &str) -> Result<(), String> {
        if self.state != MediaWorkerState::Running {
            return Err(format!("cannot {operation}: media worker has exited"));
        }
        if let Err(error) = self.writer.send_browser(&message) {
            self.mark_exited(
                format!("could not {operation}: {error}"),
                MEDIA_EXIT_PROTOCOL,
            );
            return Err(self.exit_reason.clone().unwrap_or_default());
        }
        Ok(())
    }

    fn allocate_request(&mut self) -> Result<u64, String> {
        let request_id = self.next_request;
        self.next_request = request_id
            .checked_add(1)
            .ok_or_else(|| "media request identity exhausted".to_string())?;
        Ok(request_id)
    }

    fn receive(
        &mut self,
        operation: &str,
        timeout: Duration,
    ) -> Result<WorkerMediaMessage, String> {
        match self.incoming.recv_timeout(timeout) {
            Ok(Ok(message)) => {
                self.last_progress = Instant::now();
                Ok(message)
            }
            Ok(Err(error)) => {
                self.mark_exited(
                    format!("media {operation} protocol failed: {error}"),
                    MEDIA_EXIT_PROTOCOL,
                );
                Err(self.exit_reason.clone().unwrap_or_default())
            }
            Err(error) => {
                self.mark_exited(
                    format!("media {operation} timed out or disconnected: {error}"),
                    MEDIA_EXIT_TIMEOUT,
                );
                Err(self.exit_reason.clone().unwrap_or_default())
            }
        }
    }

    fn protocol_failure<T>(&mut self, reason: &str) -> Result<T, String> {
        self.mark_exited(reason.into(), MEDIA_EXIT_PROTOCOL);
        Err(reason.into())
    }

    fn mark_exited(&mut self, reason: String, code: u32) {
        if self.state == MediaWorkerState::Running {
            terminate_job(&self.job, code);
            wait_for_process(&self.process, self.shutdown_timeout);
        }
        self.exit_reason = Some(reason);
        self.finish_exit_code();
    }

    fn finish_exit(&mut self, reason: String) {
        self.exit_reason = Some(reason);
        self.finish_exit_code();
    }

    fn finish_exit_code(&mut self) {
        self.state = MediaWorkerState::Exited;
        self.exit_code = exit_code(&self.process);
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
    }
}

impl Drop for MediaSession {
    fn drop(&mut self) {
        if self.state == MediaWorkerState::Running {
            terminate_job(&self.job, MEDIA_EXIT_DROP);
            wait_for_process(&self.process, self.shutdown_timeout);
            self.finish_exit("media worker terminated with its session".into());
        }
    }
}

fn spawn_reader(
    input: File,
    session: crate::media_protocol::MediaSessionId,
) -> Result<(Incoming, JoinHandle<()>), String> {
    let (sender, receiver) = mpsc::sync_channel(1);
    let handle = std::thread::Builder::new()
        .name("breeze-media-ipc-read".into())
        .spawn(move || {
            let mut reader = MediaFrameReader::new(input, session);
            loop {
                let message = reader.read_worker();
                let failed = message.is_err();
                if sender.send(message).is_err() || failed {
                    break;
                }
            }
        })
        .map_err(|error| format!("start media IPC reader: {error}"))?;
    Ok((receiver, handle))
}

fn validate_ready(
    expected: crate::media_protocol::Nonce,
    actual: crate::media_protocol::Nonce,
    containment: ContainmentReport,
) -> Result<(), String> {
    if expected != actual {
        return Err("media worker returned a stale bootstrap nonce".into());
    }
    if !containment.app_container {
        return Err("media worker did not start in an AppContainer".into());
    }
    if !containment.no_console_window {
        return Err("media worker unexpectedly owns a console window".into());
    }
    if !containment.minimal_environment {
        return Err("media worker inherited an unsafe process environment".into());
    }
    Ok(())
}

fn terminate_startup(process: &OwnedHandle, job: &OwnedHandle, reader: JoinHandle<()>) {
    terminate_job(job, MEDIA_EXIT_STARTUP);
    wait_for_process(process, Duration::from_secs(2));
    let _ = reader.join();
}
