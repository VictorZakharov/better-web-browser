use crate::renderer_protocol::wire::{WireReader, WireWriter};
use crate::renderer_protocol::{
    DocumentId, ProtocolError, WakeLockAction, WakeLockDisposition, WakeLockRequest, WakeLockUpdate,
};

pub(super) fn encode_request(request: &WakeLockRequest) -> Result<Vec<u8>, ProtocolError> {
    let request = request.validate()?;
    let mut writer = WireWriter::new();
    writer.u64(request.document.get());
    writer.u64(request.request_id);
    writer.u64(request.client.id);
    writer.bool(request.client.opaque);
    writer.u8(match request.action {
        WakeLockAction::Acquire => 1,
        WakeLockAction::Release => 2,
    });
    Ok(writer.finish())
}

pub(super) fn decode_request(bytes: &[u8]) -> Result<WakeLockRequest, ProtocolError> {
    let mut reader = WireReader::new(bytes);
    let request = WakeLockRequest {
        document: DocumentId::new(reader.u64()?)?,
        request_id: reader.u64()?,
        client: crate::fetch::RequestClient {
            id: reader.u64()?,
            opaque: reader.bool()?,
        },
        action: match reader.u8()? {
            1 => WakeLockAction::Acquire,
            2 => WakeLockAction::Release,
            _ => return Err(ProtocolError::InvalidPayload("wake lock action")),
        },
    };
    reader.finish()?;
    request.validate()
}

pub(super) fn encode_update(update: &WakeLockUpdate) -> Result<Vec<u8>, ProtocolError> {
    let update = update.validate()?;
    let mut writer = WireWriter::new();
    writer.u64(update.document.get());
    writer.u64(update.request_id);
    writer.u8(match update.disposition {
        WakeLockDisposition::Granted => 1,
        WakeLockDisposition::Denied => 2,
        WakeLockDisposition::Released => 3,
    });
    Ok(writer.finish())
}

pub(super) fn decode_update(bytes: &[u8]) -> Result<WakeLockUpdate, ProtocolError> {
    let mut reader = WireReader::new(bytes);
    let update = WakeLockUpdate {
        document: DocumentId::new(reader.u64()?)?,
        request_id: reader.u64()?,
        disposition: match reader.u8()? {
            1 => WakeLockDisposition::Granted,
            2 => WakeLockDisposition::Denied,
            3 => WakeLockDisposition::Released,
            _ => return Err(ProtocolError::InvalidPayload("wake lock disposition")),
        },
    };
    reader.finish()?;
    update.validate()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wake_lock_wire_validates_identity_and_direction() {
        let request = WakeLockRequest {
            document: DocumentId::new(7).unwrap(),
            request_id: 9,
            client: crate::fetch::RequestClient::default(),
            action: WakeLockAction::Acquire,
        };
        assert_eq!(
            decode_request(&encode_request(&request).unwrap()).unwrap(),
            request
        );
        let update = WakeLockUpdate {
            document: request.document,
            request_id: request.request_id,
            disposition: WakeLockDisposition::Granted,
        };
        assert_eq!(
            decode_update(&encode_update(&update).unwrap()).unwrap(),
            update
        );
        // The outer frame header, not this equal-sized payload, enforces direction.
        assert!(decode_request(&[0; 16]).is_err());
    }
}
