//! Fair, nonblocking scheduling of main-document input alongside runtime work.
use super::super::navigation::{Input, NavigationBody};
use super::*;
use crate::renderer_protocol::{CookieStateSnapshot, TransferChunk};
use crate::storage::StorageAreaKind;

pub(super) struct OutgoingNavigation {
    document: DocumentId,
    body: NavigationBody,
    offset: usize,
}

impl Broker {
    pub(super) fn begin_navigation(
        &mut self,
        start: DocumentStart,
        state: DocumentState,
        body: NavigationBody,
    ) -> Result<(), String> {
        start.validate().map_err(|error| error.to_string())?;
        state.validate().map_err(|error| error.to_string())?;
        if start.body_length != 0 {
            return Err("streaming document declared a fixed length".into());
        }
        self.outgoing_fetch.clear();
        self.resources().fetch_flow.clear();
        self.fetch_response_streaming.clear();
        self.writer()
            .send_browser(&BrowserMessage::BeginStreamingDocument(start.clone()))
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
        self.active_document = Some(start.document);
        self.document_load_deadline = Some((
            start.document,
            Instant::now() + self.resources().options.first_presentation_timeout,
        ));
        self.navigation = Some(OutgoingNavigation {
            document: start.document,
            body,
            offset: 0,
        });
        Ok(())
    }

    pub(super) fn process_navigation(&mut self) {
        for _ in 0..8 {
            if !self.writer().has_page_command_capacity() {
                break;
            }
            let Some(mut navigation) = self.navigation.take() else {
                break;
            };
            if self.active_document != Some(navigation.document) {
                break;
            }
            let message = match navigation.body.read(navigation.offset) {
                Input::Chunk(bytes) => {
                    let offset = navigation.offset as u32;
                    navigation.offset += bytes.len();
                    let document = navigation.document;
                    self.navigation = Some(navigation);
                    BrowserMessage::DocumentChunk(TransferChunk {
                        transfer_id: document.get(),
                        offset,
                        bytes,
                    })
                }
                Input::Pending => {
                    self.navigation = Some(navigation);
                    break;
                }
                Input::End => BrowserMessage::EndDocument(navigation.document),
                Input::Failed(message) => BrowserMessage::AbortDocument {
                    document: navigation.document,
                    message,
                },
            };
            if let Err(error) = self.writer().send_browser(&message) {
                self.protocol_failure(error.to_string());
                break;
            }
        }
    }
}
