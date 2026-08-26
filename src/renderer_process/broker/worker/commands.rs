//! Browser command dispatch for one renderer broker.

use super::*;

impl Broker {
    pub(super) fn process_lifecycle_commands(&mut self) {
        for _ in 0..crate::limits::MAX_QUEUED_BROWSER_COMMANDS {
            let command = match self.resources().lifecycle.try_recv() {
                Ok(command) => command,
                Err(mpsc::TryRecvError::Empty | mpsc::TryRecvError::Disconnected) => break,
            };
            match command {
                LifecycleCommand::LoadDocument { start, state, body } => {
                    if let Err(error) = self.send_document(*start, state, body) {
                        self.protocol_failure(error);
                    }
                }
                LifecycleCommand::CancelDocument(document) => self.cancel_document(document),
            }
        }
    }

    pub(super) fn process_presentation_acknowledgement(&mut self) {
        if self.resources().state_updates.has_pending() {
            return;
        }
        if !self.writer().has_page_command_capacity() {
            return;
        }
        let Some(acknowledgement) = self.resources().acknowledgements.take() else {
            return;
        };
        if self.active_document == Some(acknowledgement.document)
            && let Err(error) = self
                .writer()
                .send_browser(&BrowserMessage::PresentationAcknowledged(acknowledgement))
        {
            self.protocol_failure(error.to_string());
        }
    }

    pub(super) fn process_document_clock(&mut self) {
        if self.resources().state_updates.has_pending() {
            return;
        }
        if !self.writer().has_page_command_capacity() {
            return;
        }
        let Some(advance) = self.resources().clock.take() else {
            return;
        };
        if self.active_document == Some(advance.document) {
            self.advance_time(advance.document, advance.elapsed, advance.max_callbacks);
        }
    }

    pub(super) fn process_commands(&mut self) {
        if self.resources().state_updates.has_pending() {
            return;
        }
        for _ in 0..crate::limits::MAX_QUEUED_BROWSER_COMMANDS {
            if !self.writer().has_page_command_capacity() {
                break;
            }
            let command = match self.resources().commands.try_recv() {
                Ok(command) => command,
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.begin_shutdown(None);
                    break;
                }
            };
            match command {
                BrokerCommand::Ping(reply) => self.send_ping(Some(reply)),
                BrokerCommand::ProbeRestrictions {
                    loopback_port,
                    reply,
                } => self.probe_restrictions(loopback_port, reply),
                BrokerCommand::Test(command) => {
                    if let Err(error) = self.writer().send_browser(&BrowserMessage::Test(command)) {
                        self.protocol_failure(error.to_string());
                    }
                }
                BrokerCommand::ViewportChanged { document, viewport } => {
                    if self.active_document == Some(document)
                        && let Err(error) = self
                            .writer()
                            .send_browser(&BrowserMessage::ViewportChanged { document, viewport })
                    {
                        self.protocol_failure(error.to_string());
                    }
                }
                BrokerCommand::Input(input) => {
                    if self.active_document == Some(input.document())
                        && let Err(error) =
                            self.writer().send_browser(&BrowserMessage::Input(input))
                    {
                        self.protocol_failure(error.to_string());
                    }
                }
                BrokerCommand::Shutdown(reply) => self.begin_shutdown(Some(reply)),
                BrokerCommand::Terminate => {
                    self.exit_reason = Some(RendererExitReason::Terminated);
                    self.terminate_job(74);
                }
                BrokerCommand::CloseJobForTest(reply) => self.close_job_for_test(reply),
            }
        }
    }

    pub(super) fn process_state_updates(&mut self) {
        if self.outgoing_state_update.is_none() {
            while let Some(update) = self.resources().state_updates.take() {
                if self.active_document == Some(update.document()) {
                    self.outgoing_state_update = Some(OutgoingStateUpdate::from(update));
                    break;
                }
                self.resources().state_updates.complete();
            }
        }
        for _ in 0..crate::limits::MAX_QUEUED_BROWSER_COMMANDS {
            if !self.writer().has_page_command_capacity() {
                break;
            }
            let Some(message) = self
                .outgoing_state_update
                .as_mut()
                .and_then(|outgoing| outgoing.messages.pop_front())
            else {
                self.outgoing_state_update = None;
                self.resources().state_updates.complete();
                break;
            };
            let completed = self
                .outgoing_state_update
                .as_ref()
                .is_some_and(|outgoing| outgoing.messages.is_empty());
            if let Err(error) = self.writer().send_browser(&message) {
                self.protocol_failure(error);
                break;
            }
            if completed {
                self.outgoing_state_update = None;
                self.resources().state_updates.complete();
                break;
            }
        }
    }

    fn probe_restrictions(
        &mut self,
        loopback_port: u16,
        reply: mpsc::Sender<Result<RestrictionReport, String>>,
    ) {
        if self.pending_probe.is_some() {
            let _ = reply.send(Err("renderer probe already pending".into()));
        } else if self
            .writer()
            .send_browser(&BrowserMessage::Test(TestCommand::ProbeRestrictions {
                loopback_port,
            }))
            .is_ok()
        {
            self.pending_probe = Some(reply);
        } else {
            let _ = reply.send(Err("send renderer restriction probe".into()));
        }
    }

    fn advance_time(&mut self, document: DocumentId, elapsed: Duration, max_callbacks: u32) {
        let elapsed_micros = elapsed.as_micros().min(u64::MAX as u128) as u64;
        if let Err(error) = self.writer().send_browser(&BrowserMessage::AdvanceTime {
            document,
            elapsed_micros,
            max_callbacks,
        }) {
            self.protocol_failure(error.to_string());
        }
    }

    fn cancel_document(&mut self, document: DocumentId) {
        // A completed event can already be waiting for the browser while cancellation crosses the
        // command pipe. It has no authority after replacement and must not consume the new
        // document's bounded event capacity.
        self.resources().events.discard_document(document);
        self.resources().state_updates.discard_document(document);
        if self
            .outgoing_state_update
            .as_ref()
            .is_some_and(|update| update.document == document)
        {
            self.outgoing_state_update = None;
            self.resources().state_updates.complete();
        }
        if self.active_document == Some(document) {
            self.active_document = None;
            self.document_load_deadline = None;
            self.retired_document = Some(document);
            self.outgoing_fetch.clear();
            self.fetch_response_streaming.clear();
        }
        if let Err(error) = self
            .writer()
            .send_browser(&BrowserMessage::CancelDocument(document))
        {
            self.protocol_failure(error.to_string());
        }
    }

    fn close_job_for_test(&mut self, reply: mpsc::Sender<Result<(), String>>) {
        if !self.resources().options.test_mode {
            let _ = reply.send(Err("Job close is restricted to test sessions".into()));
            return;
        }
        self.exit_reason = Some(RendererExitReason::Terminated);
        let job = self
            .resources
            .as_mut()
            .and_then(|resources| resources.job.take());
        drop(job);
        let _ = reply.send(Ok(()));
    }
}

impl OutgoingStateUpdate {
    fn from(update: super::super::state_updates::StateUpdate) -> Self {
        use super::super::state_updates::StateUpdate;
        use crate::renderer_protocol::{
            StorageSnapshotEnd, StorageSnapshotEntry, StorageSnapshotStart,
        };

        let document = update.document();
        let messages = match update {
            StateUpdate::Cookie(snapshot) => {
                VecDeque::from([BrowserMessage::CookieSnapshot(snapshot)])
            }
            StateUpdate::Storage {
                document,
                area,
                snapshot,
            } => {
                let version = snapshot.version;
                let entry_count = u32::try_from(snapshot.entries.len())
                    .expect("validated storage entry count fits the wire protocol");
                let mut messages = VecDeque::with_capacity(snapshot.entries.len() + 2);
                messages.push_back(BrowserMessage::StorageSnapshotStart(StorageSnapshotStart {
                    document,
                    area,
                    version,
                    entry_count,
                }));
                messages.extend(snapshot.entries.into_iter().map(|entry| {
                    BrowserMessage::StorageSnapshotEntry(StorageSnapshotEntry {
                        document,
                        area,
                        entry,
                    })
                }));
                messages.push_back(BrowserMessage::StorageSnapshotEnd(StorageSnapshotEnd {
                    document,
                    area,
                    version,
                }));
                messages
            }
        };
        Self { document, messages }
    }
}
