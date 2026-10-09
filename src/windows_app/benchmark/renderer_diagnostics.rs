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

pub(super) fn memory_to_json(snapshots: &[&RendererSnapshot]) -> String {
    let records = snapshots.iter().map(|snapshot| {
        let memory = &snapshot.memory_evidence;
        json!({
            "process_id": snapshot.process_id,
            "session_id": snapshot.session_id,
            "state": format!("{:?}", snapshot.state),
            "scope": "renderer_process_including_native_gpu",
            "sampling_interval_ms": 1000,
            "current_sample_available": memory.current_sample_available,
            "successful_samples": memory.successful_samples,
            "last_private_bytes": memory.last_private_bytes,
            "last_working_set_bytes": memory.last_working_set_bytes,
            "observed_peak_private_bytes": memory.observed_peak_private_bytes,
            "retained_peak_working_set_bytes": memory.retained_peak_working_set_bytes,
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

    fn memory_snapshot() -> RendererSnapshot {
        RendererSnapshot {
            process_id: 42,
            session_id: 2,
            context_id: 1,
            state: better_web_browser::renderer_process::RendererState::Running,
            working_set: 10,
            private_memory: 11,
            peak_working_set: 12,
            memory_evidence: Default::default(),
            cpu_ticks: 0,
            handle_count: 0,
            uptime: Duration::from_secs(3),
            last_pong_age: Duration::ZERO,
            active_task: None,
            active_task_elapsed: None,
            queues: Default::default(),
            pending_state_updates: 0,
            submitted_state_updates: 0,
            coalesced_state_updates: 0,
            exit_reason: None,
            exit: None,
        }
    }

    #[test]
    fn unknown_memory_is_null_not_a_successful_zero_sample() {
        let snapshot = memory_snapshot();
        let records: Value = serde_json::from_str(&memory_to_json(&[&snapshot])).unwrap();
        let record = &records[0];
        assert_eq!(record["current_sample_available"], false);
        assert_eq!(record["successful_samples"], 0);
        for field in [
            "last_private_bytes",
            "last_working_set_bytes",
            "observed_peak_private_bytes",
            "retained_peak_working_set_bytes",
        ] {
            assert!(record[field].is_null(), "unknown {field} became zero");
        }
        assert_eq!(record["scope"], "renderer_process_including_native_gpu");
        assert_eq!(record["sampling_interval_ms"], 1000);
    }

    #[test]
    fn exited_memory_observations_retain_peaks_without_claiming_current_usage() {
        use better_web_browser::renderer_process::{RendererMemoryEvidence, RendererState};
        let mut snapshot = memory_snapshot();
        snapshot.state = RendererState::Exited;
        snapshot.private_memory = 0;
        snapshot.working_set = 0;
        snapshot.memory_evidence = RendererMemoryEvidence {
            current_sample_available: false,
            successful_samples: 4,
            last_private_bytes: Some(100),
            last_working_set_bytes: Some(80),
            observed_peak_private_bytes: Some(150),
            retained_peak_working_set_bytes: Some(130),
        };
        let records: Value = serde_json::from_str(&memory_to_json(&[&snapshot])).unwrap();
        let record = &records[0];
        assert_eq!(record["state"], "Exited");
        assert_eq!(record["current_sample_available"], false);
        assert_eq!(record["last_private_bytes"], 100);
        assert_eq!(record["observed_peak_private_bytes"], 150);
        assert_eq!(record["retained_peak_working_set_bytes"], 130);
        // This field is separate from report.rs's current live-process totals.
        assert!(record.get("private_bytes").is_none());
    }

    #[test]
    fn memory_observations_keep_independent_sessions_and_an_empty_list() {
        let first = memory_snapshot();
        let mut second = memory_snapshot();
        second.process_id = 43;
        second.session_id = 3;
        second.memory_evidence.last_private_bytes = Some(0);
        second.memory_evidence.current_sample_available = true;
        second.memory_evidence.successful_samples = 1;
        let records: Value = serde_json::from_str(&memory_to_json(&[&first, &second])).unwrap();
        assert_eq!(records.as_array().unwrap().len(), 2);
        assert_eq!(records[0]["session_id"], 2);
        assert!(records[0]["last_private_bytes"].is_null());
        assert_eq!(records[1]["session_id"], 3);
        assert_eq!(records[1]["last_private_bytes"], 0);
        assert_eq!(memory_to_json(&[]), "[]");
    }
}
