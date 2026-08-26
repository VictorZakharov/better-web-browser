//! Renderer endpoint state machine over the two inherited anonymous pipes.

mod fetch;
mod runtime;
mod state;

pub(in crate::renderer_process::child) use self::fetch::PendingFetchBatch;
use self::fetch::state::FetchState;
pub(in crate::renderer_process::child) use self::fetch::state::ScriptFetchDelivery;
use self::state::{IncomingDocumentState, IncomingStorageUpdate};
use super::document::{DocumentRuntime, RendererTextSystem};
use super::handle_test;
use crate::limits::MAX_RENDERER_PRESENTATION_BYTES;
use crate::renderer_protocol::{
    BrowserMessage, DocumentId, DocumentStart, FrameReader, FrameWriter, ProtocolError,
    RENDERER_DIAGNOSTIC_INTERNAL_ERROR, RENDERER_DIAGNOSTIC_PROTOCOL_ERROR, RendererDiagnostic,
    RendererMessage, RendererPresentation, TestCommand, TransferAssembler, TransferChunk,
};
use std::collections::VecDeque;
use std::fs::File;

pub(super) struct ChildConnection {
    reader: FrameReader<File>,
    writer: FrameWriter<File>,
    test_mode: bool,
    stopping: bool,
    pending: VecDeque<BrowserMessage>,
    pending_fetch_deliveries: VecDeque<ScriptFetchDelivery>,
    fetches: FetchState,
    incoming_document: Option<IncomingDocument>,
    incoming_storage_update: Option<IncomingStorageUpdate>,
    document: Option<DocumentRuntime>,
    // Browser-authoritative corrections can already be in the pipe when DocumentFailed retires
    // the runtime. Keep its identity long enough to validate and drain those transfers.
    failed_document: Option<DocumentId>,
    prepared_text: Option<RendererTextSystem>,
    next_request_id: u64,
    next_batch_id: u64,
}

impl ChildConnection {
    pub(super) fn new(
        reader: FrameReader<File>,
        writer: FrameWriter<File>,
        test_mode: bool,
        text: RendererTextSystem,
    ) -> Self {
        Self {
            reader,
            writer,
            test_mode,
            stopping: false,
            pending: VecDeque::new(),
            pending_fetch_deliveries: VecDeque::new(),
            fetches: FetchState::default(),
            incoming_document: None,
            incoming_storage_update: None,
            document: None,
            failed_document: None,
            // Ready is sent only after this renderer-owned dependency is initialized. Otherwise
            // the browser can submit a document to a process that is not actually command-ready.
            prepared_text: Some(text),
            next_request_id: 1,
            next_batch_id: 1,
        }
    }

    pub(super) fn run(mut self) -> Result<(), String> {
        while !self.stopping {
            if let Some(delivery) = self.pending_fetch_deliveries.pop_front() {
                self.deliver_script_fetch(delivery)?;
                continue;
            }
            let message = match self.pending.pop_front() {
                Some(message) => Ok(message),
                None => self.reader.read_browser(),
            };
            let message = match message {
                Ok(message) => message,
                Err(error) => {
                    self.send_protocol_diagnostic(&error);
                    return Err(error.to_string());
                }
            };
            if let Err(error) = self.handle(message) {
                self.send_diagnostic(RENDERER_DIAGNOSTIC_INTERNAL_ERROR, &error);
                return Err(error);
            }
        }
        Ok(())
    }

    pub(super) fn allocate_request_id(&mut self) -> u64 {
        let id = self.next_request_id;
        self.next_request_id = id.checked_add(1).unwrap_or(1);
        id
    }

    fn handle(&mut self, message: BrowserMessage) -> Result<(), String> {
        match message {
            BrowserMessage::Ping(token) => self
                .writer
                .send_renderer(&RendererMessage::Pong(token))
                .map_err(|error| error.to_string()),
            BrowserMessage::Shutdown => self.shutdown(),
            BrowserMessage::ProtocolFailure(_) => Err("browser rejected renderer IPC".into()),
            BrowserMessage::Test(TestCommand::DocumentError) if self.test_mode => {
                let document =
                    self.document
                        .as_ref()
                        .map(DocumentRuntime::id)
                        .ok_or_else(|| {
                            "document error injection requires an active document".to_string()
                        })?;
                self.send_document_failure(document, "injected document error".into())
            }
            BrowserMessage::Test(command) if self.test_mode => {
                handle_test(command, &mut self.writer)
            }
            BrowserMessage::Test(_) => Err("test command rejected".into()),
            BrowserMessage::BeginDocument(start) => self.begin_document(start),
            BrowserMessage::DocumentChunk(chunk) => self.document_chunk(chunk),
            BrowserMessage::EndDocument(document) => self.finish_document(document),
            BrowserMessage::CookieSnapshot(snapshot) => self.document_state_cookie(snapshot),
            BrowserMessage::StorageSnapshotStart(start) => self.document_state_start(start),
            BrowserMessage::StorageSnapshotEntry(entry) => self.document_state_entry(entry),
            BrowserMessage::StorageSnapshotEnd(end) => self.document_state_end(end),
            BrowserMessage::AdvanceTime {
                document,
                elapsed_micros,
                max_callbacks,
            } => self.advance_document(document, elapsed_micros, max_callbacks),
            BrowserMessage::ViewportChanged { document, viewport } => {
                self.resize_document(document, viewport)
            }
            BrowserMessage::Input(input) => self.interact_document(input),
            BrowserMessage::PresentationAcknowledged(acknowledgement) => {
                self.acknowledge_presentation(acknowledgement)
            }
            BrowserMessage::CancelDocument(document) => {
                self.cancel_document_fetches(document);
                if self
                    .incoming_storage_update
                    .as_ref()
                    .is_some_and(|update| update.document() == document)
                {
                    self.incoming_storage_update = None;
                }
                if self
                    .document
                    .as_ref()
                    .is_some_and(|runtime| runtime.id() == document)
                {
                    self.prepared_text = self.document.take().map(DocumentRuntime::into_text);
                }
                Ok(())
            }
            message @ (BrowserMessage::FetchResponseStart(_)
            | BrowserMessage::FetchResponseChunk(_)
            | BrowserMessage::FetchResponseEnd(_)
            | BrowserMessage::FetchResponseAbort(_)) => {
                if let Some(delivery) = self.handle_fetch_message(message)? {
                    self.deliver_script_fetch(delivery)?;
                }
                Ok(())
            }
            BrowserMessage::Hello { .. } => Err("duplicate renderer Hello".into()),
        }
    }

    fn send_presentation(&mut self, presentation: &RendererPresentation) -> Result<(), String> {
        let encode_started = std::time::Instant::now();
        let bytes = presentation.encode().map_err(|error| error.to_string())?;
        let encode_micros = encode_started
            .elapsed()
            .as_micros()
            .min(u128::from(u64::MAX)) as u64;
        if bytes.len() > MAX_RENDERER_PRESENTATION_BYTES {
            return Err("renderer presentation exceeded its byte budget".into());
        }
        let total_length = u32::try_from(bytes.len())
            .map_err(|_| "renderer presentation length overflow".to_string())?;
        self.writer
            .send_renderer(&RendererMessage::PresentationStart {
                document: presentation.document,
                revision: presentation.revision,
                total_length,
                encode_micros,
            })
            .map_err(|error| error.to_string())?;
        self.send_renderer_chunks(
            presentation.revision,
            &bytes,
            RendererMessage::PresentationChunk,
        )?;
        self.writer
            .send_renderer(&RendererMessage::PresentationEnd {
                document: presentation.document,
                revision: presentation.revision,
            })
            .map_err(|error| error.to_string())
    }

    fn send_renderer_chunks(
        &mut self,
        transfer_id: u64,
        bytes: &[u8],
        message: impl Fn(TransferChunk) -> RendererMessage,
    ) -> Result<(), String> {
        const CHUNK_BYTES: usize = 1024 * 1024;
        for (index, chunk) in bytes.chunks(CHUNK_BYTES).enumerate() {
            let offset = u32::try_from(index * CHUNK_BYTES)
                .map_err(|_| "renderer transfer offset overflow".to_string())?;
            self.writer
                .send_renderer(&message(TransferChunk {
                    transfer_id,
                    offset,
                    bytes: chunk.to_vec(),
                }))
                .map_err(|error| error.to_string())?;
        }
        Ok(())
    }

    fn send_document_failure(
        &mut self,
        document: DocumentId,
        detail: String,
    ) -> Result<(), String> {
        self.document.take();
        self.failed_document = Some(document);
        let detail = bounded_detail(&detail);
        self.writer
            .send_renderer(&RendererMessage::DocumentFailed { document, detail })
            .map_err(|error| error.to_string())
    }

    fn shutdown(&mut self) -> Result<(), String> {
        self.document.take();
        self.incoming_document.take();
        self.writer
            .send_renderer(&RendererMessage::ShutdownComplete)
            .map_err(|error| error.to_string())?;
        self.stopping = true;
        Ok(())
    }

    fn send_protocol_diagnostic(&mut self, error: &ProtocolError) {
        self.send_diagnostic(RENDERER_DIAGNOSTIC_PROTOCOL_ERROR, &error.to_string());
    }

    fn send_diagnostic(&mut self, code: u16, detail: &str) {
        if let Ok(diagnostic) = RendererDiagnostic::new(code, bounded_detail(detail)) {
            let _ = self
                .writer
                .send_renderer(&RendererMessage::Diagnostic(diagnostic));
        }
    }
}

struct IncomingDocument {
    start: DocumentStart,
    state: IncomingDocumentState,
    body: TransferAssembler,
}

fn bounded_detail(detail: &str) -> String {
    const MAX: usize = 16 * 1024;
    let (detail, truncated) = crate::limits::bounded_utf8_prefix(detail, MAX);
    if truncated {
        format!("{detail}…")
    } else {
        detail.to_string()
    }
}
