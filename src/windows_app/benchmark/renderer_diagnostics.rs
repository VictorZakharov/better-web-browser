//! JSON diagnostics for renderer liveness and bounded broker queues.

use better_web_browser::renderer_process::{RendererExit, RendererSnapshot};
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

pub(super) fn exits_to_json(exits: &[&RendererExit]) -> String {
    let records = exits.iter().map(|exit| {
        json!({
            "process_id": exit.process_id,
            "code": exit.code,
            "code_hex": format!("{:#x}", exit.code),
            "reason": format!("{:?}", exit.reason),
            "uptime_ms": milliseconds(exit.uptime),
        })
    });
    serde_json::to_string(&Value::Array(records.collect())).unwrap_or_else(|_| "[]".into())
}

pub(super) fn first_failure(exits: &[&RendererExit]) -> Option<String> {
    exits.iter().find_map(|exit| {
        exit.crash_surface()
            .map(|surface| format!("{}: {}", surface.title, surface.detail))
    })
}

fn milliseconds(duration: std::time::Duration) -> f64 {
    duration.as_secs_f64() * 1_000.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use better_web_browser::renderer_process::RendererExitReason;
    use std::time::Duration;

    #[test]
    fn exited_renderer_is_serialized_and_fails_the_benchmark() {
        let exit = RendererExit {
            process_id: 42,
            code: 0xc000_0017,
            reason: RendererExitReason::Crash,
            uptime: Duration::from_secs(9),
        };
        let exits = [&exit];

        let json = exits_to_json(&exits);
        assert!(json.contains("\"code_hex\":\"0xc0000017\""));
        assert!(json.contains("\"uptime_ms\":9000.0"));
        assert!(first_failure(&exits).is_some_and(|error| error.contains("exit 0xc0000017")));
    }
}
