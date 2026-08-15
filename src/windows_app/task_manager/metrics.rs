use super::super::process_metrics::MemorySample;
use super::super::renderer_lifecycle::{RendererLifecyclePhase, RendererTaskStatus};
use super::super::{format_bytes, format_duration};
use better_web_browser::branding::PRODUCT_NAME;

pub(super) struct TaskMetricsView {
    pub(super) cpu: String,
    pub(super) working_set: String,
    pub(super) process_summary: String,
    pub(super) processes: Vec<ProcessRowView>,
    pub(super) active_requests: String,
    pub(super) pages_completed: String,
    pub(super) failed_loads: String,
    pub(super) downloaded: String,
    pub(super) last_parse: String,
    pub(super) draw_items: String,
}

impl Default for TaskMetricsView {
    fn default() -> Self {
        Self {
            cpu: "0.0%".into(),
            working_set: "—".into(),
            process_summary: "1 LIVE".into(),
            processes: Vec::new(),
            active_requests: "0".into(),
            pages_completed: "0".into(),
            failed_loads: "0".into(),
            downloaded: "0 B".into(),
            last_parse: "0 μs".into(),
            draw_items: "0".into(),
        }
    }
}

pub(super) struct ProcessRowView {
    pub(super) depth: usize,
    pub(super) name: String,
    pub(super) detail: String,
    pub(super) note: String,
    pub(super) cpu: String,
    pub(super) working_set: String,
    pub(super) private_memory: String,
    pub(super) handles: String,
}

pub(super) fn browser_process_row(
    cpu_percent: f64,
    memory: &MemorySample,
    handles: u32,
    uptime: std::time::Duration,
) -> ProcessRowView {
    ProcessRowView {
        depth: 0,
        name: format!("{PRODUCT_NAME} Browser"),
        detail: format!("PID {} · privileged broker", std::process::id()),
        note: format!(
            "Uptime {} · peak working set {}",
            format_duration(uptime),
            format_bytes(memory.peak_working_set as u64)
        ),
        cpu: format!("{cpu_percent:.1}%"),
        working_set: format_bytes(memory.working_set as u64),
        private_memory: format_bytes(memory.private_usage as u64),
        handles: handles.to_string(),
    }
}

pub(super) fn renderer_process_row(
    status: &RendererTaskStatus,
    cpu_percent: f64,
) -> ProcessRowView {
    let phase = phase_label(status.phase);
    let detail = status
        .snapshot
        .as_ref()
        .map(|snapshot| {
            format!(
                "PID {} · session {} · {phase} · restarts {}",
                snapshot.process_id, snapshot.session_id, status.restart_count
            )
        })
        .unwrap_or_else(|| format!("PID pending · {phase} · restarts {}", status.restart_count));
    let note = renderer_note(status);
    let (working_set, private_memory, handles) = status
        .snapshot
        .as_ref()
        .map(|snapshot| {
            (
                format_bytes(snapshot.working_set as u64),
                format_bytes(snapshot.private_memory as u64),
                snapshot.handle_count.to_string(),
            )
        })
        .unwrap_or_else(|| ("—".into(), "—".into(), "—".into()));
    ProcessRowView {
        depth: 1,
        name: format!("Renderer · Context {}", status.context_id),
        detail,
        note,
        cpu: format!("{cpu_percent:.1}%"),
        working_set,
        private_memory,
        handles,
    }
}

pub(super) fn renderer_is_live(status: &RendererTaskStatus) -> bool {
    matches!(
        status.phase,
        RendererLifecyclePhase::Running | RendererLifecyclePhase::Unresponsive
    )
}

fn phase_label(phase: RendererLifecyclePhase) -> &'static str {
    match phase {
        RendererLifecyclePhase::Starting => "STARTING",
        RendererLifecyclePhase::Running => "RUNNING",
        RendererLifecyclePhase::Unresponsive => "UNRESPONSIVE",
        RendererLifecyclePhase::Exited => "EXITED",
        RendererLifecyclePhase::LaunchFailed => "LAUNCH FAILED",
    }
}

fn renderer_note(status: &RendererTaskStatus) -> String {
    match status.phase {
        RendererLifecyclePhase::Exited => {
            if let Some(exit) = status.last_exit.as_ref() {
                return format!("Last exit: {:?} ({:#x})", exit.reason, exit.code);
            }
        }
        RendererLifecyclePhase::LaunchFailed => {
            if let Some(error) = status.launch_error.as_ref() {
                return format!("Launch error: {error}");
            }
        }
        RendererLifecyclePhase::Running | RendererLifecyclePhase::Unresponsive => {
            if let Some(diagnostic) = status.last_diagnostic.as_ref() {
                return format!("Diagnostic: {diagnostic}");
            }
        }
        RendererLifecyclePhase::Starting => {}
    }
    status
        .snapshot
        .as_ref()
        .map(|snapshot| {
            format!(
                "Uptime {} · last pong {} ago",
                format_duration(snapshot.uptime),
                format_duration(snapshot.last_pong_age)
            )
        })
        .unwrap_or_else(|| "Waiting for the bounded startup handshake".into())
}
