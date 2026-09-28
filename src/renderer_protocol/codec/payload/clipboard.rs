//! Bounded, typed plain-text Clipboard IPC. Clipboard bytes never travel in diagnostics.

use crate::renderer_protocol::clipboard::MAX_CLIPBOARD_TEXT_BYTES;
use crate::renderer_protocol::wire::{WireReader, WireWriter};
use crate::renderer_protocol::{
    ClipboardAction, ClipboardError, ClipboardRequest, ClipboardUpdate, ClipboardValue, DocumentId,
    ProtocolError,
};

pub(super) fn encode_request(request: &ClipboardRequest) -> Result<Vec<u8>, ProtocolError> {
    request.validate()?;
    let mut writer = WireWriter::new();
    writer.u64(request.document.get());
    writer.u64(request.request_id);
    writer.u64(request.client.id);
    writer.bool(request.client.opaque);
    match &request.action {
        ClipboardAction::ReadText => writer.u8(1),
        ClipboardAction::WriteText(text) => {
            writer.u8(2);
            writer.string(text)?;
        }
    }
    Ok(writer.finish())
}

pub(super) fn decode_request(bytes: &[u8]) -> Result<ClipboardRequest, ProtocolError> {
    let mut reader = WireReader::new(bytes);
    let document = DocumentId::new(reader.u64()?)?;
    let request_id = reader.u64()?;
    let client = crate::fetch::RequestClient {
        id: reader.u64()?,
        opaque: reader.bool()?,
    };
    let action = match reader.u8()? {
        1 => ClipboardAction::ReadText,
        2 => ClipboardAction::WriteText(reader.string(MAX_CLIPBOARD_TEXT_BYTES)?),
        _ => return Err(ProtocolError::InvalidPayload("clipboard action")),
    };
    reader.finish()?;
    let request = ClipboardRequest {
        document,
        request_id,
        client,
        action,
    };
    request.validate()?;
    Ok(request)
}

pub(super) fn encode_update(update: &ClipboardUpdate) -> Result<Vec<u8>, ProtocolError> {
    update.validate()?;
    let mut writer = WireWriter::new();
    writer.u64(update.document.get());
    writer.u64(update.request_id);
    match &update.result {
        Ok(ClipboardValue::Text(text)) => {
            writer.u8(1);
            writer.string(text)?;
        }
        Ok(ClipboardValue::Written) => writer.u8(2),
        Err(ClipboardError::NotAllowed) => writer.u8(3),
        Err(ClipboardError::NotFound) => writer.u8(4),
        Err(ClipboardError::NotReadable) => writer.u8(5),
    }
    Ok(writer.finish())
}

pub(super) fn decode_update(bytes: &[u8]) -> Result<ClipboardUpdate, ProtocolError> {
    let mut reader = WireReader::new(bytes);
    let document = DocumentId::new(reader.u64()?)?;
    let request_id = reader.u64()?;
    let result = match reader.u8()? {
        1 => Ok(ClipboardValue::Text(
            reader.string(MAX_CLIPBOARD_TEXT_BYTES)?,
        )),
        2 => Ok(ClipboardValue::Written),
        3 => Err(ClipboardError::NotAllowed),
        4 => Err(ClipboardError::NotFound),
        5 => Err(ClipboardError::NotReadable),
        _ => return Err(ProtocolError::InvalidPayload("clipboard result")),
    };
    reader.finish()?;
    let update = ClipboardUpdate {
        document,
        request_id,
        result,
    };
    update.validate()?;
    Ok(update)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::renderer_protocol::{
        BrowserMessage, FrameReader, FrameWriter, RendererMessage, RendererSessionId,
    };
    use std::io::Cursor;

    fn request(action: ClipboardAction) -> ClipboardRequest {
        ClipboardRequest {
            document: DocumentId::new(7).unwrap(),
            request_id: 42,
            client: crate::fetch::RequestClient {
                id: 0,
                opaque: false,
            },
            action,
        }
    }

    #[test]
    fn text_requests_and_replies_round_trip_without_unbounded_payloads() {
        for action in [
            ClipboardAction::ReadText,
            ClipboardAction::WriteText("A\r\nB 🦊".into()),
        ] {
            let item = request(action);
            assert_eq!(
                decode_request(&encode_request(&item).unwrap()).unwrap(),
                item
            );
        }
        let update = ClipboardUpdate {
            document: DocumentId::new(7).unwrap(),
            request_id: 42,
            result: Ok(ClipboardValue::Text("A\r\nB 🦊".into())),
        };
        assert_eq!(
            decode_update(&encode_update(&update).unwrap()).unwrap(),
            update
        );
        for result in [
            Ok(ClipboardValue::Written),
            Err(ClipboardError::NotAllowed),
            Err(ClipboardError::NotFound),
            Err(ClipboardError::NotReadable),
        ] {
            let update = ClipboardUpdate {
                result,
                ..update.clone()
            };
            assert_eq!(
                decode_update(&encode_update(&update).unwrap()).unwrap(),
                update
            );
        }
        let oversized = request(ClipboardAction::WriteText(
            "x".repeat(MAX_CLIPBOARD_TEXT_BYTES + 1),
        ));
        assert!(encode_request(&oversized).is_err());
        let mut truncated = encode_request(&request(ClipboardAction::ReadText)).unwrap();
        truncated.pop();
        assert!(decode_request(&truncated).is_err());
    }

    #[test]
    fn maximum_text_fits_the_framed_ipc_budget_in_both_directions() {
        let session = RendererSessionId::new(91).unwrap();
        let text = "x".repeat(MAX_CLIPBOARD_TEXT_BYTES);
        let request =
            RendererMessage::ClipboardRequest(request(ClipboardAction::WriteText(text.clone())));
        let mut bytes = Vec::new();
        FrameWriter::new(&mut bytes, session)
            .send_renderer(&request)
            .unwrap();
        assert_eq!(u16::from_le_bytes(bytes[8..10].try_into().unwrap()), 0x01f0);
        assert_eq!(
            FrameReader::new(Cursor::new(bytes), session)
                .read_renderer()
                .unwrap(),
            request
        );

        let update = BrowserMessage::ClipboardUpdate(ClipboardUpdate {
            document: DocumentId::new(7).unwrap(),
            request_id: 42,
            result: Ok(ClipboardValue::Text(text)),
        });
        let mut bytes = Vec::new();
        FrameWriter::new(&mut bytes, session)
            .send_browser(&update)
            .unwrap();
        assert_eq!(u16::from_le_bytes(bytes[8..10].try_into().unwrap()), 0x01f1);
        assert_eq!(
            FrameReader::new(Cursor::new(bytes), session)
                .read_browser()
                .unwrap(),
            update
        );
    }
}
