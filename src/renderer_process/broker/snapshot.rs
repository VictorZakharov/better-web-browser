//! Public lifecycle/diagnostic snapshots, separate from session construction.
use super::{RendererExit, RendererExitReason, RendererMemoryEvidence, RendererQueueDepths};
use std::time::Duration;

impl RendererExitReason {
    pub fn task_timeout(&self) -> Option<&super::RendererTaskTimeout> {
        match self {
            Self::TaskBudgetExceeded(timeout) => Some(timeout),
            _ => None,
        }
    }
}

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
    pub memory_evidence: RendererMemoryEvidence,
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
