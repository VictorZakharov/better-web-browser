use super::{RendererSnapshot, RendererState};
use crate::renderer_process::windows::ProcessSample;
use crate::renderer_protocol::{BrowsingContextId, RendererSessionId};
use std::time::{Duration, Instant};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RendererExitReason {
    CleanShutdown,
    Crash,
    InternalFailure(String),
    ProtocolFailure(String),
    ShutdownTimeout,
    TaskBudgetExceeded(RendererTaskTimeout),
    Terminated,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RendererQueueDepths {
    pub browser_commands: usize,
    pub renderer_commands: usize,
    pub renderer_messages: usize,
    pub browser_events: usize,
    pub state_updates: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RendererTaskTimeout {
    pub task: String,
    pub elapsed: Duration,
    pub queues: RendererQueueDepths,
}

impl std::fmt::Display for RendererTaskTimeout {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "{} for {:.0} ms; queue depths: browser commands {}, renderer commands {}, renderer messages {}, browser events {}, state updates {}",
            self.task,
            self.elapsed.as_secs_f64() * 1_000.0,
            self.queues.browser_commands,
            self.queues.renderer_commands,
            self.queues.renderer_messages,
            self.queues.browser_events,
            self.queues.state_updates,
        )
    }
}

#[derive(Clone, Debug)]
pub struct RendererExit {
    pub process_id: u32,
    pub code: u32,
    pub reason: RendererExitReason,
    pub uptime: Duration,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RendererCrashSurface {
    pub title: String,
    pub detail: String,
    pub can_reload: bool,
}

impl RendererExit {
    pub fn crash_surface(&self) -> Option<RendererCrashSurface> {
        let reason = match &self.reason {
            RendererExitReason::CleanShutdown => return None,
            RendererExitReason::Crash => "crashed".to_string(),
            RendererExitReason::InternalFailure(error) => {
                format!("stopped after an internal error: {error}")
            }
            RendererExitReason::ProtocolFailure(error) => {
                format!("violated the IPC protocol: {error}")
            }
            RendererExitReason::ShutdownTimeout => "did not shut down in time".to_string(),
            RendererExitReason::TaskBudgetExceeded(timeout) => {
                format!("exceeded its unresponsive-task budget while {timeout}")
            }
            RendererExitReason::Terminated => "was terminated by the browser".to_string(),
        };
        Some(RendererCrashSurface {
            title: "Renderer stopped".into(),
            detail: format!(
                "Renderer process {} {reason} (exit {:#x})",
                self.process_id, self.code,
            ),
            can_reload: true,
        })
    }
}

pub(super) struct SharedDiagnostics {
    pub(super) process_id: u32,
    pub(super) session: RendererSessionId,
    pub(super) context: BrowsingContextId,
    pub(super) state: RendererState,
    pub(super) sample: ProcessSample,
    pub(super) started: Instant,
    pub(super) last_pong: Instant,
    pub(super) active_task: Option<String>,
    pub(super) active_task_started: Option<Instant>,
    pub(super) exit_reason: Option<RendererExitReason>,
    pub(super) exit: Option<RendererExit>,
}

impl SharedDiagnostics {
    pub(super) fn snapshot(&self) -> RendererSnapshot {
        let now = Instant::now();
        RendererSnapshot {
            process_id: self.process_id,
            session_id: self.session.get(),
            context_id: self.context.get(),
            state: self.state,
            working_set: self.sample.working_set,
            private_memory: self.sample.private_memory,
            peak_working_set: self.sample.peak_working_set,
            cpu_ticks: self.sample.cpu_ticks,
            handle_count: self.sample.handle_count,
            uptime: now.saturating_duration_since(self.started),
            last_pong_age: now.saturating_duration_since(self.last_pong),
            active_task: self.active_task.clone(),
            active_task_elapsed: self
                .active_task_started
                .map(|started| now.saturating_duration_since(started)),
            queues: RendererQueueDepths::default(),
            pending_state_updates: 0,
            submitted_state_updates: 0,
            coalesced_state_updates: 0,
            exit_reason: self.exit_reason.clone(),
            exit: self.exit.clone(),
        }
    }
}
