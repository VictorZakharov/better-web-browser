//! Heartbeat and task deadlines for the renderer control plane.

use super::*;

impl Broker {
    fn queue_depths(&self) -> RendererQueueDepths {
        RendererQueueDepths {
            browser_commands: self
                .resources()
                .command_depth
                .pending()
                .saturating_add(self.resources().viewport.pending()),
            renderer_commands: self.writer().pending(),
            renderer_messages: self.resources().incoming_depth.pending(),
            browser_events: self.resources().events.pending(),
            state_updates: self.resources().state_updates.pending(),
        }
    }

    pub(super) fn enforce_deadlines(&mut self) {
        let now = Instant::now();
        if self
            .shutdown_deadline
            .is_some_and(|deadline| now >= deadline)
        {
            self.exit_reason = Some(RendererExitReason::ShutdownTimeout);
            self.terminate_job(73);
            self.shutdown_deadline = None;
        }
        let last_pong = self.shared().last_pong;
        let unresponsive = now.saturating_duration_since(last_pong)
            >= self.resources().options.unresponsive_timeout;
        if unresponsive && self.shared().state == RendererState::Running {
            self.shared().state = RendererState::Unresponsive;
            if let Err(error) = self.emit_event(RendererEvent::Unresponsive) {
                self.protocol_failure(error.to_string());
            }
        }
        let options = &self.resources().options;
        if task_budget_exceeded(
            now,
            last_pong,
            self.document_load_deadline.map(|(_, deadline)| deadline),
            options.unresponsive_timeout,
            options.unresponsive_kill_timeout,
        ) && self.shared().state == RendererState::Unresponsive
            && self.exit_reason.is_none()
        {
            let (task, started) = {
                let shared = self.shared();
                (
                    shared
                        .active_task
                        .clone()
                        .unwrap_or_else(|| "processing an unidentified renderer task".into()),
                    shared.active_task_started.unwrap_or(shared.last_pong),
                )
            };
            self.exit_reason = Some(RendererExitReason::TaskBudgetExceeded(
                RendererTaskTimeout {
                    task,
                    elapsed: now.saturating_duration_since(started),
                    queues: self.queue_depths(),
                },
            ));
            self.terminate_job(74);
        }
    }

    pub(super) fn send_heartbeat(&mut self) {
        if self.shutdown_deadline.is_some()
            || self.last_heartbeat.elapsed() < self.resources().options.heartbeat_interval
        {
            return;
        }
        self.last_heartbeat = Instant::now();
        self.send_ping(None);
    }

    pub(super) fn send_ping(&mut self, reply: Option<mpsc::Sender<Result<(), String>>>) {
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
}

fn task_budget_exceeded(
    now: Instant,
    last_pong: Instant,
    document_load_deadline: Option<Instant>,
    unresponsive_timeout: Duration,
    unresponsive_kill_timeout: Duration,
) -> bool {
    document_load_deadline
        .map(|deadline| now >= deadline)
        .unwrap_or_else(|| {
            now.saturating_duration_since(last_pong)
                >= unresponsive_timeout.saturating_add(unresponsive_kill_timeout)
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn document_load_uses_its_first_presentation_deadline() {
        let started = Instant::now();
        let ordinary_budget = Duration::from_millis(450);
        let load_deadline = started + Duration::from_secs(2);
        assert!(!task_budget_exceeded(
            started + ordinary_budget,
            started,
            Some(load_deadline),
            Duration::from_millis(300),
            Duration::from_millis(150),
        ));
        assert!(task_budget_exceeded(
            load_deadline,
            started,
            Some(load_deadline),
            Duration::from_millis(300),
            Duration::from_millis(150),
        ));
    }

    #[test]
    fn ordinary_renderer_work_keeps_the_short_heartbeat_budget() {
        let started = Instant::now();
        assert!(task_budget_exceeded(
            started + Duration::from_millis(450),
            started,
            None,
            Duration::from_millis(300),
            Duration::from_millis(150),
        ));
    }

    #[test]
    fn production_renderer_allows_a_finite_slow_task_to_recover() {
        let started = Instant::now();
        let unresponsive = crate::limits::RENDERER_UNRESPONSIVE_TIMEOUT;
        let recovery = crate::limits::RENDERER_UNRESPONSIVE_KILL_TIMEOUT;

        assert!(!task_budget_exceeded(
            started + Duration::from_secs(5),
            started,
            None,
            unresponsive,
            recovery,
        ));
        assert!(task_budget_exceeded(
            started + unresponsive + recovery,
            started,
            None,
            unresponsive,
            recovery,
        ));
    }
}
