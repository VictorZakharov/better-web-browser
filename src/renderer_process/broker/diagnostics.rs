use super::{RendererSnapshot, RendererState};
use crate::renderer_process::windows::ProcessSample;
use crate::renderer_protocol::{BrowsingContextId, RendererSessionId};
use std::time::{Duration, Instant};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RendererExitReason {
    CleanShutdown,
    Crash,
    ProtocolFailure(String),
    ShutdownTimeout,
    TaskBudgetExceeded,
    Terminated,
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
            RendererExitReason::ProtocolFailure(error) => {
                format!("violated the IPC protocol: {error}")
            }
            RendererExitReason::ShutdownTimeout => "did not shut down in time".to_string(),
            RendererExitReason::TaskBudgetExceeded => {
                "exceeded its unresponsive-task budget".to_string()
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
            exit_reason: self.exit_reason.clone(),
            exit: self.exit.clone(),
        }
    }
}
