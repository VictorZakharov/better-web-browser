//! Dispatch incoming renderer protocol frames without blocking producers or the UI.
use super::*;

impl Broker {
    pub(super) fn process_messages(&mut self) {
        for _ in 0..crate::limits::MAX_QUEUED_RENDERER_IPC_MESSAGES {
            let message = match self.resources().incoming.try_recv() {
                Ok(message) => {
                    self.resources().incoming_depth.finish_dequeue();
                    message
                }
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => break,
            };
            match message {
                Ok(RendererMessage::VideoFrame(chunk)) => self.process_video_chunk(chunk),
                Ok(RendererMessage::Pong(token)) => {
                    // Token zero acknowledges completed work or independently bounded progress.
                    // Real Ping tokens start at one and retain reply routing.
                    {
                        let mut shared = self.shared();
                        shared.last_pong = Instant::now();
                        shared.state = RendererState::Running;
                        shared.active_task = None;
                        shared.active_task_started = None;
                    }
                    if token != 0
                        && let Some(reply) = self.pending_pings.remove(&token)
                    {
                        let _ = reply.send(Ok(()));
                    }
                }
                Ok(RendererMessage::ShutdownComplete) => {
                    if self.shutdown_deadline.is_some() {
                        self.shutdown_acknowledged = true;
                    } else {
                        self.protocol_failure("unsolicited renderer shutdown completion".into());
                    }
                }
                Ok(RendererMessage::Diagnostic(diagnostic)) => {
                    if diagnostic.code == crate::renderer_protocol::RENDERER_DIAGNOSTIC_TASK_STARTED
                    {
                        let mut shared = self.shared();
                        shared.last_pong = Instant::now();
                        shared.state = RendererState::Running;
                        shared.active_task = Some(diagnostic.text);
                        shared.active_task_started = Some(Instant::now());
                        continue;
                    }
                    if diagnostic.code == crate::renderer_protocol::RENDERER_DIAGNOSTIC_TASK_STAGE {
                        self.shared().active_task = Some(diagnostic.text);
                        continue;
                    }
                    if self.exit_reason.is_none() {
                        self.exit_reason = match diagnostic.code {
                            crate::renderer_protocol::RENDERER_DIAGNOSTIC_INTERNAL_ERROR => {
                                Some(RendererExitReason::InternalFailure(diagnostic.text.clone()))
                            }
                            crate::renderer_protocol::RENDERER_DIAGNOSTIC_PROTOCOL_ERROR => {
                                Some(RendererExitReason::ProtocolFailure(diagnostic.text.clone()))
                            }
                            _ => None,
                        };
                    }
                    if let Err(error) = self.emit_event(RendererEvent::Diagnostic {
                        code: diagnostic.code,
                        text: diagnostic.text,
                    }) {
                        self.protocol_failure(error.to_string());
                        break;
                    }
                }
                Ok(RendererMessage::Restrictions(report)) => {
                    if let Some(reply) = self.pending_probe.take() {
                        let _ = reply.send(Ok(report));
                    } else {
                        self.protocol_failure("unsolicited renderer restriction report".into());
                    }
                }
                Ok(
                    message @ (RendererMessage::FetchBatchStart { .. }
                    | RendererMessage::FetchRequestStart { .. }
                    | RendererMessage::FetchRequestChunk(_)
                    | RendererMessage::FetchRequestEnd(_)
                    | RendererMessage::FetchRequestAbort { .. }
                    | RendererMessage::FetchResponseConsumed { .. }
                    | RendererMessage::PresentationStart { .. }
                    | RendererMessage::PresentationChunk(_)
                    | RendererMessage::PresentationEnd { .. }
                    | RendererMessage::RuntimeUpdate(_)
                    | RendererMessage::DocumentFailed { .. }
                    | RendererMessage::NavigationRequested { .. }
                    | RendererMessage::PointerCursor(_)
                    | RendererMessage::FullscreenRequest(_)
                    | RendererMessage::CookieMutation(_)
                    | RendererMessage::StorageMutation(_)
                    | RendererMessage::WebSocketCommand(_)
                    | RendererMessage::StateSnapshotApplied(_)),
                ) => {
                    if let Err(error) = self.process_document_message(message) {
                        self.protocol_failure(error.to_string());
                    }
                }
                Ok(RendererMessage::Ready { .. }) => {
                    self.protocol_failure("duplicate renderer Ready".into());
                }
                Err(_) if self.shutdown_acknowledged => break,
                Err(ProtocolError::Io(error)) => {
                    if wait_for_process(&self.resources().process, Duration::from_millis(100)) {
                        break;
                    }
                    self.protocol_failure(format!("renderer IPC closed unexpectedly: {error}"));
                }
                Err(error) => self.protocol_failure(error.to_string()),
            }
        }
    }
}
