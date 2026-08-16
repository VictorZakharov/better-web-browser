use super::diagnostics::{RendererExit, RendererExitReason, SharedDiagnostics};
use super::{RendererEvent, RendererState};
use crate::renderer_process::launcher::RendererLaunchOptions;
use crate::renderer_process::windows::{
    exit_code, process_exited, process_sample, terminate_job, wait_for_process,
};
use crate::renderer_protocol::{
    BrowserMessage, FrameWriter, ProtocolError, RendererMessage, RestrictionReport, TestCommand,
};
use std::collections::HashMap;
use std::fs::File;
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
    Shutdown(mpsc::Sender<Result<RendererExit, String>>),
    Terminate,
    CloseJobForTest(mpsc::Sender<Result<(), String>>),
}

pub(super) struct BrokerResources {
    pub(super) process: OwnedHandle,
    pub(super) job: Option<OwnedHandle>,
    pub(super) writer: FrameWriter<File>,
    pub(super) incoming: mpsc::Receiver<Result<RendererMessage, ProtocolError>>,
    pub(super) reader_thread: JoinHandle<()>,
    pub(super) commands: mpsc::Receiver<BrokerCommand>,
    pub(super) events: mpsc::Sender<RendererEvent>,
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
    exit_reason: Option<RendererExitReason>,
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
            exit_reason: None,
        }
    }

    fn run(&mut self) {
        loop {
            self.process_commands();
            self.process_messages();
            if self.process_has_exited() {
                self.finish_exit();
                break;
            }
            self.enforce_deadlines();
            self.send_heartbeat();
            self.refresh_metrics();
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    fn process_commands(&mut self) {
        loop {
            let command = match self.resources().commands.try_recv() {
                Ok(command) => command,
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.begin_shutdown(None);
                    break;
                }
            };
            match command {
                BrokerCommand::Ping(reply) => self.send_ping(Some(reply)),
                BrokerCommand::ProbeRestrictions {
                    loopback_port,
                    reply,
                } => {
                    if self.pending_probe.is_some() {
                        let _ = reply.send(Err("renderer probe already pending".into()));
                    } else if self
                        .writer()
                        .send_browser(&BrowserMessage::Test(TestCommand::ProbeRestrictions {
                            loopback_port,
                        }))
                        .is_ok()
                    {
                        self.pending_probe = Some(reply);
                    } else {
                        let _ = reply.send(Err("send renderer restriction probe".into()));
                    }
                }
                BrokerCommand::Test(command) => {
                    if let Err(error) = self.writer().send_browser(&BrowserMessage::Test(command)) {
                        self.protocol_failure(error.to_string());
                    }
                }
                BrokerCommand::Shutdown(reply) => self.begin_shutdown(Some(reply)),
                BrokerCommand::Terminate => {
                    self.exit_reason = Some(RendererExitReason::Terminated);
                    self.terminate_job(74);
                }
                BrokerCommand::CloseJobForTest(reply) => {
                    if !self.resources().options.test_mode {
                        let _ = reply.send(Err("Job close is restricted to test sessions".into()));
                    } else {
                        self.exit_reason = Some(RendererExitReason::Terminated);
                        let job = self
                            .resources
                            .as_mut()
                            .and_then(|resources| resources.job.take());
                        drop(job);
                        let _ = reply.send(Ok(()));
                    }
                }
            }
        }
    }

    fn process_messages(&mut self) {
        loop {
            match self.resources().incoming.try_recv() {
                Ok(Ok(RendererMessage::Pong(token))) => {
                    self.shared().last_pong = Instant::now();
                    self.shared().state = RendererState::Running;
                    if let Some(reply) = self.pending_pings.remove(&token) {
                        let _ = reply.send(Ok(()));
                    }
                }
                Ok(Ok(RendererMessage::ShutdownComplete)) => {
                    if self.shutdown_deadline.is_some() {
                        self.shutdown_acknowledged = true;
                    } else {
                        self.protocol_failure("unsolicited renderer shutdown completion".into());
                    }
                }
                Ok(Ok(RendererMessage::Diagnostic(diagnostic))) => {
                    let _ = self.resources().events.send(RendererEvent::Diagnostic {
                        code: diagnostic.code,
                        text: diagnostic.text,
                    });
                }
                Ok(Ok(RendererMessage::Restrictions(report))) => {
                    if let Some(reply) = self.pending_probe.take() {
                        let _ = reply.send(Ok(report));
                    } else {
                        self.protocol_failure("unsolicited renderer restriction report".into());
                    }
                }
                Ok(Ok(RendererMessage::Ready { .. })) => {
                    self.protocol_failure("duplicate renderer Ready".into());
                }
                Ok(Err(_)) if self.shutdown_acknowledged => break,
                Ok(Err(ProtocolError::Io(error))) => {
                    if wait_for_process(&self.resources().process, Duration::from_millis(100)) {
                        break;
                    }
                    self.protocol_failure(format!("renderer IPC closed unexpectedly: {error}"));
                }
                Ok(Err(error)) => self.protocol_failure(error.to_string()),
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => break,
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

    fn enforce_deadlines(&mut self) {
        let now = Instant::now();
        if self
            .shutdown_deadline
            .is_some_and(|deadline| now >= deadline)
        {
            self.exit_reason = Some(RendererExitReason::ShutdownTimeout);
            self.terminate_job(73);
            self.shutdown_deadline = None;
        }
        let unresponsive = now.saturating_duration_since(self.shared().last_pong)
            >= self.resources().options.unresponsive_timeout;
        if unresponsive && self.shared().state == RendererState::Running {
            self.shared().state = RendererState::Unresponsive;
            let _ = self.resources().events.send(RendererEvent::Unresponsive);
        }
        let task_budget = self
            .resources()
            .options
            .unresponsive_timeout
            .saturating_add(self.resources().options.unresponsive_kill_timeout);
        if now.saturating_duration_since(self.shared().last_pong) >= task_budget
            && self.shared().state == RendererState::Unresponsive
            && self.exit_reason.is_none()
        {
            self.exit_reason = Some(RendererExitReason::TaskBudgetExceeded);
            self.terminate_job(74);
        }
    }

    fn send_heartbeat(&mut self) {
        if self.shutdown_deadline.is_some()
            || self.last_heartbeat.elapsed() < self.resources().options.heartbeat_interval
        {
            return;
        }
        self.last_heartbeat = Instant::now();
        self.send_ping(None);
    }

    fn send_ping(&mut self, reply: Option<mpsc::Sender<Result<(), String>>>) {
        let token = self.next_ping;
        self.next_ping = self.next_ping.checked_add(1).unwrap_or(1);
        match self.writer().send_browser(&BrowserMessage::Ping(token)) {
            Ok(()) => {
                if let Some(reply) = reply {
                    self.pending_pings.insert(token, reply);
                }
            }
            Err(error) => {
                if let Some(reply) = reply {
                    let _ = reply.send(Err(error.to_string()));
                }
                self.protocol_failure(error.to_string());
            }
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
        let reason = self.exit_reason.clone().unwrap_or(default_reason);
        let uptime = resources.shared.lock().unwrap().started.elapsed();
        {
            let mut shared = resources.shared.lock().unwrap();
            shared.state = RendererState::Exited;
            shared.sample = process_sample(&resources.process);
            shared.exit_reason = Some(reason.clone());
        }
        let exit = RendererExit {
            process_id: resources.shared.lock().unwrap().process_id,
            code,
            reason,
            uptime,
        };
        fail_pending(&mut self.pending_pings, &mut self.pending_probe);
        if let Some(reply) = self.shutdown_reply.take() {
            let _ = reply.send(Ok(exit.clone()));
        }
        let _ = resources.events.send(RendererEvent::Exited(exit));
        drop(resources.writer);
        drop(resources.job);
        drop(resources.process);
        let _ = resources.reader_thread.join();
    }

    fn resources(&self) -> &BrokerResources {
        self.resources.as_ref().expect("broker resources available")
    }

    fn writer(&mut self) -> &mut FrameWriter<File> {
        &mut self
            .resources
            .as_mut()
            .expect("broker resources available")
            .writer
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
