//! JSON diagnostics for renderer liveness and bounded broker queues.

use better_web_browser::renderer_process::RendererSnapshot;
use serde_json::{Value, json};

pub(super) fn to_json(snapshots: &[&RendererSnapshot]) -> String {
    let records = snapshots.iter().map(|snapshot| {
        let queues = &snapshot.queues;
        json!({
            "process_id": snapshot.process_id,
            "session_id": snapshot.session_id,
            "context_id": snapshot.context_id,
            "state": format!("{:?}", snapshot.state),
            "last_pong_age_ms": milliseconds(snapshot.last_pong_age),
            "active_task": snapshot.active_task,
            "active_task_elapsed_ms": snapshot.active_task_elapsed.map(milliseconds),
            "queue_depths": {
                "browser_commands": queues.browser_commands,
                "renderer_commands": queues.renderer_commands,
                "renderer_messages": queues.renderer_messages,
                "browser_events": queues.browser_events,
                "state_updates": queues.state_updates,
            },
            "exit_reason": snapshot.exit_reason.as_ref().map(|reason| format!("{reason:?}")),
        })
    });
    serde_json::to_string(&Value::Array(records.collect())).unwrap_or_else(|_| "[]".into())
}

fn milliseconds(duration: std::time::Duration) -> f64 {
    duration.as_secs_f64() * 1_000.0
}
