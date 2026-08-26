//! Process control, event polling, and diagnostic snapshots for a renderer session.

use super::*;

impl RendererSession {
    pub fn snapshot(&self) -> RendererSnapshot {
        let state_updates = self.state_updates.snapshot();
        let mut snapshot = self
            .shared
            .lock()
            .expect("renderer diagnostics lock poisoned")
            .snapshot();
        snapshot.pending_state_updates = state_updates.pending;
        snapshot.submitted_state_updates = state_updates.submitted;
        snapshot.coalesced_state_updates = state_updates.coalesced;
        snapshot.queues = RendererQueueDepths {
            browser_commands: self.command_depth.pending(),
            renderer_commands: self.outbound_diagnostics.pending(),
            renderer_messages: self.incoming_depth.pending(),
            browser_events: self.events.pending(),
            state_updates: state_updates.pending,
        };
        snapshot
    }

    pub fn ping(&self, timeout: Duration) -> Result<(), String> {
        let (reply, response) = mpsc::channel();
        self.send_command(worker::BrokerCommand::Ping(reply))?;
        response
            .recv_timeout(timeout)
            .map_err(|_| "renderer ping timed out".to_string())?
    }

    pub fn probe_restrictions(
        &self,
        loopback_port: u16,
        timeout: Duration,
    ) -> Result<RestrictionReport, String> {
        let (reply, response) = mpsc::channel();
        self.send_command(worker::BrokerCommand::ProbeRestrictions {
            loopback_port,
            reply,
        })?;
        response
            .recv_timeout(timeout)
            .map_err(|_| "renderer restriction probe timed out".to_string())?
    }

    pub fn send_test_command(&self, command: TestCommand) -> Result<(), String> {
        self.send_command(worker::BrokerCommand::Test(command))
    }

    pub fn wait_for_event(&self, timeout: Duration) -> Result<RendererEvent, String> {
        self.events
            .recv_timeout(timeout)
            .map_err(|_| "renderer event timed out".to_string())
    }

    pub fn try_event(&self) -> Result<Option<RendererEvent>, String> {
        match self.events.try_recv() {
            Ok(event) => Ok(Some(event)),
            Err(mpsc::TryRecvError::Empty) => Ok(None),
            Err(mpsc::TryRecvError::Disconnected) => Err("renderer broker has exited".into()),
        }
    }

    pub fn wait_for_exit(&self, timeout: Duration) -> Result<RendererExit, String> {
        let deadline = Instant::now() + timeout;
        loop {
            if let Some(exit) = self.snapshot().exit {
                return Ok(exit);
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err("renderer exit timed out".into());
            }
            if let RendererEvent::Exited(exit) = self.wait_for_event(remaining)? {
                return Ok(exit);
            }
        }
    }

    pub fn terminate(&self) -> Result<(), String> {
        let mut shared = self
            .shared
            .lock()
            .map_err(|_| "renderer diagnostics lock poisoned".to_string())?;
        if shared.state == RendererState::Exited {
            return Err("renderer has already exited".into());
        }
        terminate_job_checked(&self.termination_job, 74)
            .map_err(|error| format!("terminate renderer Job: {error}"))?;
        shared.exit_reason = Some(RendererExitReason::Terminated);
        drop(shared);
        self.wake.notify();
        Ok(())
    }

    /// Requests immediate renderer termination without waiting on the caller's thread.
    ///
    /// Browser UI recovery paths use this instead of `Drop`, whose graceful shutdown wait is
    /// intentionally bounded but can still take several seconds for an unresponsive renderer.
    pub fn terminate_in_background(mut self) {
        self.events.close();
        // Termination is an ownership boundary, so end the Job before returning to the caller.
        // Queueing a broker command here lets a saturated renderer overlap its replacement.
        let _ = self.terminate();
        let Some(worker) = self.worker.take() else {
            return;
        };
        let wake = self.wake.clone();
        let _ = std::thread::Builder::new()
            .name("breeze-renderer-reaper".into())
            .spawn(move || {
                wake.notify();
                let _ = worker.join();
            });
    }

    #[doc(hidden)]
    pub fn close_job_for_test(&self) -> Result<(), String> {
        let (reply, response) = mpsc::channel();
        self.send_command(worker::BrokerCommand::CloseJobForTest(reply))?;
        response
            .recv_timeout(Duration::from_secs(1))
            .map_err(|_| "renderer Job close timed out".to_string())?
    }

    pub fn shutdown(&mut self) -> Result<RendererExit, String> {
        self.events.close();
        let (reply, response) = mpsc::channel();
        self.send_blocking_command(worker::BrokerCommand::Shutdown(reply))?;
        let result = response
            .recv_timeout(self.shutdown_timeout + Duration::from_secs(1))
            .map_err(|_| "renderer shutdown timed out".to_string())?;
        self.join_worker();
        result
    }

    fn join_worker(&mut self) {
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

impl Drop for RendererSession {
    fn drop(&mut self) {
        if self.worker.is_none() {
            return;
        }
        self.events.close();
        let (reply, response) = mpsc::channel();
        let _ = self.send_blocking_command(worker::BrokerCommand::Shutdown(reply));
        if response
            .recv_timeout(self.shutdown_timeout + Duration::from_secs(1))
            .is_err()
        {
            let _ = self.send_blocking_command(worker::BrokerCommand::Terminate);
        }
        self.join_worker();
    }
}
