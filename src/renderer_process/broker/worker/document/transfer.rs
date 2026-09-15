//! Initial document and snapshot transfer encoding.
use super::*;

impl Broker {
    pub(in crate::renderer_process::broker::worker) fn send_document(
        &mut self,
        start: DocumentStart,
        state: DocumentState,
        body: Vec<u8>,
    ) -> Result<(), String> {
        start.validate().map_err(|error| error.to_string())?;
        state.validate().map_err(|error| error.to_string())?;
        if body.len() != start.body_length as usize {
            return Err("document transfer length does not match its declaration".into());
        }
        self.outgoing_fetch.clear();
        self.resources().fetch_flow.clear();
        self.fetch_response_streaming.clear();
        self.writer()
            .send_browser(&BrowserMessage::BeginDocument(start.clone()))
            .map_err(|error| error.to_string())?;
        self.send_cookie_snapshot(CookieStateSnapshot {
            document: start.document,
            version: state.cookie_version,
            header: state.cookie_header,
        })?;
        self.send_storage_snapshot(start.document, StorageAreaKind::Local, state.local_storage)?;
        self.send_storage_snapshot(
            start.document,
            StorageAreaKind::Session,
            state.session_storage,
        )?;
        self.send_browser_chunks(start.document.get(), &body, BrowserMessage::DocumentChunk)?;
        self.writer()
            .send_browser(&BrowserMessage::EndDocument(start.document))
            .map_err(|error| error.to_string())?;
        self.active_document = Some(start.document);
        self.document_load_deadline = Some((
            start.document,
            Instant::now() + self.resources().options.first_presentation_timeout,
        ));
        Ok(())
    }

    pub(in crate::renderer_process::broker::worker) fn send_cookie_snapshot(
        &mut self,
        snapshot: CookieStateSnapshot,
    ) -> Result<(), String> {
        snapshot.validate().map_err(|error| error.to_string())?;
        self.writer()
            .send_browser(&BrowserMessage::CookieSnapshot(snapshot))
            .map_err(|error| error.to_string())
    }

    pub(in crate::renderer_process::broker::worker) fn send_storage_snapshot(
        &mut self,
        document: DocumentId,
        area: StorageAreaKind,
        snapshot: StorageAreaSnapshot,
    ) -> Result<(), String> {
        snapshot.validate().map_err(|error| error.to_string())?;
        let version = snapshot.version;
        let entry_count = u32::try_from(snapshot.entries.len())
            .map_err(|_| "storage snapshot entry count overflow".to_string())?;
        self.writer()
            .send_browser(&BrowserMessage::StorageSnapshotStart(
                StorageSnapshotStart {
                    document,
                    area,
                    version,
                    entry_count,
                },
            ))
            .map_err(|error| error.to_string())?;
        for entry in snapshot.entries {
            self.writer()
                .send_browser(&BrowserMessage::StorageSnapshotEntry(
                    StorageSnapshotEntry {
                        document,
                        area,
                        entry,
                    },
                ))
                .map_err(|error| error.to_string())?;
        }
        self.writer()
            .send_browser(&BrowserMessage::StorageSnapshotEnd(StorageSnapshotEnd {
                document,
                area,
                version,
            }))
            .map_err(|error| error.to_string())
    }

    pub(super) fn send_browser_chunks(
        &mut self,
        transfer_id: u64,
        bytes: &[u8],
        message: impl Fn(TransferChunk) -> BrowserMessage,
    ) -> Result<(), String> {
        const CHUNK_BYTES: usize = 1024 * 1024;
        for (index, chunk) in bytes.chunks(CHUNK_BYTES).enumerate() {
            let offset = u32::try_from(index * CHUNK_BYTES)
                .map_err(|_| "IPC transfer offset overflow".to_string())?;
            self.writer()
                .send_browser(&message(TransferChunk {
                    transfer_id,
                    offset,
                    bytes: chunk.to_vec(),
                }))
                .map_err(|error| error.to_string())?;
        }
        Ok(())
    }
}
