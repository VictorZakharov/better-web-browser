//! Preserve the original renderer state before containment tears down the session.
use super::*;

impl BrowserTab {
    pub(super) fn retain_failure_snapshot(&mut self, snapshot: RendererSnapshot) {
        if let Some(exit) = snapshot.exit.as_ref() {
            self.incidents.record(
                "renderer",
                format!(
                    "process {} exited {:#x}: {:?}",
                    exit.process_id, exit.code, exit.reason
                ),
            );
        }
        // The last monitor tick may still say Running. Never replace the actual exit with
        // the subsequent browser-owned termination used to contain this failed page.
        self.last_renderer_snapshot = Some(snapshot);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use better_web_browser::renderer_process::{RendererExit, RendererExitReason, RendererState};

    fn snapshot() -> RendererSnapshot {
        RendererSnapshot {
            process_id: 42,
            session_id: 2,
            context_id: 1,
            state: RendererState::Running,
            working_set: 10,
            private_memory: 11,
            peak_working_set: 12,
            cpu_ticks: 13,
            handle_count: 14,
            uptime: Duration::from_secs(3),
            last_pong_age: Duration::from_millis(50),
            active_task: Some("layout".into()),
            active_task_elapsed: Some(Duration::from_millis(50)),
            queues: Default::default(),
            pending_state_updates: 0,
            submitted_state_updates: 0,
            coalesced_state_updates: 0,
            exit_reason: None,
            exit: None,
        }
    }

    #[test]
    fn containment_preserves_terminal_evidence_instead_of_the_previous_running_tick() {
        let mut tab = BrowserTab::new(TabId::first());
        tab.last_renderer_snapshot = Some(snapshot());
        let mut terminal = snapshot();
        let reason = RendererExitReason::ProtocolFailure("original frame failure".into());
        terminal.state = RendererState::Exited;
        terminal.exit_reason = Some(reason.clone());
        terminal.exit = Some(RendererExit {
            process_id: 42,
            code: 72,
            reason: reason.clone(),
            uptime: terminal.uptime,
        });
        tab.retain_failure_snapshot(terminal);
        tab.mark_crashed("input channel disconnected".into());
        let retained = tab.last_renderer_snapshot.as_ref().unwrap();
        assert_eq!(retained.state, RendererState::Exited);
        assert_eq!(retained.exit_reason.as_ref(), Some(&reason));
        assert_eq!(retained.exit.as_ref().unwrap().code, 72);
        let report = tab.incidents.report();
        assert!(report.contains("process 42 exited 0x48"));
        assert!(report.contains("original frame failure"));
    }

    #[test]
    fn containment_does_not_invent_an_exit_if_the_renderer_was_still_running() {
        let mut tab = BrowserTab::new(TabId::first());
        tab.retain_failure_snapshot(snapshot());
        tab.mark_crashed("invalid browser-side presentation".into());
        let retained = tab.last_renderer_snapshot.as_ref().unwrap();
        assert_eq!(retained.active_task.as_deref(), Some("layout"));
        assert!(retained.exit.is_none());
        assert!(retained.exit_reason.is_none());
        assert!(!tab.incidents.report().contains("exited"));
    }
}
