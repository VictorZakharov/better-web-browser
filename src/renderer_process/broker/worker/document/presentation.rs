//! Incoming presentation transfer ownership and decoding.
use super::*;
impl Broker {
    pub(super) fn begin_presentation(
        &mut self,
        document: DocumentId,
        revision: u64,
        total_length: u32,
        encode_micros: u64,
    ) -> Result<(), ProtocolError> {
        let owns_document =
            self.active_document == Some(document) || self.retired_document == Some(document);
        if self.incoming_presentation.is_some() || !owns_document || revision == 0 {
            return Err(ProtocolError::InvalidPayload("nested presentation"));
        }
        if self.active_document == Some(document) {
            self.document_load_deadline = None;
            // See the equivalent Fetch transfer rule above. Once replacement output arrives, the
            // retired document can no longer have unread frames on the renderer pipe.
            self.retired_document = None;
        }
        self.incoming_presentation = Some(IncomingPresentation {
            document,
            revision,
            encode_micros,
            body: TransferAssembler::new(
                revision,
                total_length as usize,
                MAX_RENDERER_PRESENTATION_BYTES,
            )?,
        });
        Ok(())
    }

    pub(super) fn finish_presentation(
        &mut self,
        document: DocumentId,
        revision: u64,
    ) -> Result<(), ProtocolError> {
        let incoming = self
            .incoming_presentation
            .take()
            .ok_or(ProtocolError::InvalidPayload(
                "unsolicited presentation end",
            ))?;
        if incoming.document != document || incoming.revision != revision {
            return Err(ProtocolError::InvalidPayload("presentation identity"));
        }
        let decode_started = std::time::Instant::now();
        let mut presentation = RendererPresentation::decode(&incoming.body.finish(revision)?)?;
        presentation.load.presentation_encode_micros = incoming.encode_micros;
        presentation.load.presentation_decode_micros = decode_started
            .elapsed()
            .as_micros()
            .min(u128::from(u64::MAX))
            as u64;
        if presentation.document != document || presentation.revision != revision {
            return Err(ProtocolError::InvalidPayload(
                "presentation archive identity",
            ));
        }
        if self.active_document == Some(document) {
            self.emit_event(RendererEvent::Presentation(Box::new(presentation)))?;
        }
        Ok(())
    }
}
