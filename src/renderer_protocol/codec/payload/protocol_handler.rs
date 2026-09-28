use crate::fetch::RequestClient;
use crate::renderer_protocol::wire::{WireReader, WireWriter};
use crate::renderer_protocol::{
    DocumentId, ProtocolError, ProtocolHandlerAction, ProtocolHandlerRequest,
};

pub(super) fn encode_request(request: &ProtocolHandlerRequest) -> Result<Vec<u8>, ProtocolError> {
    request.validate()?;
    let mut writer = WireWriter::new();
    writer.u64(request.document.get());
    writer.u64(request.client.id);
    writer.bool(request.client.opaque);
    writer.u8(match request.action {
        ProtocolHandlerAction::Register => 1,
        ProtocolHandlerAction::Unregister => 2,
    });
    writer.string(&request.scheme)?;
    writer.string(&request.template)?;
    Ok(writer.finish())
}

pub(super) fn decode_request(bytes: &[u8]) -> Result<ProtocolHandlerRequest, ProtocolError> {
    let mut reader = WireReader::new(bytes);
    let document = DocumentId::new(reader.u64()?)?;
    let client = RequestClient {
        id: reader.u64()?,
        opaque: reader.bool()?,
    };
    let action = match reader.u8()? {
        1 => ProtocolHandlerAction::Register,
        2 => ProtocolHandlerAction::Unregister,
        _ => return Err(ProtocolError::InvalidPayload("protocol handler action")),
    };
    let request = ProtocolHandlerRequest {
        document,
        client,
        action,
        scheme: reader.string(32)?,
        template: reader.string(crate::protocol_handlers::MAX_HANDLER_TEMPLATE_BYTES)?,
    };
    reader.finish()?;
    request.validate()?;
    Ok(request)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::renderer_protocol::{FrameReader, FrameWriter, RendererMessage, RendererSessionId};

    #[test]
    fn request_round_trips_and_rejects_oversized_or_invalid_actions() {
        let request = ProtocolHandlerRequest {
            document: DocumentId::new(7).unwrap(),
            client: RequestClient {
                id: 0,
                opaque: false,
            },
            action: ProtocolHandlerAction::Register,
            scheme: "web+soup".into(),
            template: "https://example.test/?uri=%s".into(),
        };
        let bytes = encode_request(&request).unwrap();
        assert_eq!(decode_request(&bytes).unwrap(), request);
        let session = RendererSessionId::new(9).unwrap();
        let mut writer = FrameWriter::new(Vec::new(), session);
        writer
            .send_renderer(&RendererMessage::ProtocolHandlerRequest(request.clone()))
            .unwrap();
        let frame = writer.into_inner();
        assert_eq!(
            FrameReader::new(frame.as_slice(), session)
                .read_renderer()
                .unwrap(),
            RendererMessage::ProtocolHandlerRequest(request.clone())
        );
        let mut invalid = bytes;
        invalid[17] = 9;
        assert!(decode_request(&invalid).is_err());
        let mut oversized = request;
        oversized.template = "x".repeat(crate::protocol_handlers::MAX_HANDLER_TEMPLATE_BYTES + 1);
        assert!(encode_request(&oversized).is_err());
    }
}
