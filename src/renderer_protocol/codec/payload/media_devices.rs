use crate::fetch::RequestClient;
use crate::renderer_protocol::wire::{WireReader, WireWriter};
use crate::renderer_protocol::{
    DocumentId, MediaDeviceError, MediaDeviceRequest, MediaDeviceResult, MediaDeviceUpdate,
    ProtocolError,
};

pub(super) fn encode_request(request: &MediaDeviceRequest) -> Result<Vec<u8>, ProtocolError> {
    request.validate()?;
    let mut writer = WireWriter::new();
    writer.u64(request.document.get());
    writer.u64(request.request_id);
    writer.u64(request.client.id);
    writer.bool(request.client.opaque);
    Ok(writer.finish())
}

pub(super) fn decode_request(bytes: &[u8]) -> Result<MediaDeviceRequest, ProtocolError> {
    let mut reader = WireReader::new(bytes);
    let request = MediaDeviceRequest {
        document: DocumentId::new(reader.u64()?)?,
        request_id: reader.u64()?,
        client: RequestClient {
            id: reader.u64()?,
            opaque: reader.bool()?,
        },
    };
    reader.finish()?;
    request.validate()?;
    Ok(request)
}

pub(super) fn encode_update(update: &MediaDeviceUpdate) -> Result<Vec<u8>, ProtocolError> {
    update.validate()?;
    let mut writer = WireWriter::new();
    writer.u64(update.document.get());
    writer.u64(update.request_id);
    match update.result {
        MediaDeviceResult::Presence { microphone, camera } => {
            writer.u8(1);
            writer.bool(microphone);
            writer.bool(camera);
        }
        MediaDeviceResult::Error(MediaDeviceError::NotAllowed) => writer.u8(2),
        MediaDeviceResult::Error(MediaDeviceError::NotReadable) => writer.u8(3),
    }
    Ok(writer.finish())
}

pub(super) fn decode_update(bytes: &[u8]) -> Result<MediaDeviceUpdate, ProtocolError> {
    let mut reader = WireReader::new(bytes);
    let document = DocumentId::new(reader.u64()?)?;
    let request_id = reader.u64()?;
    let result = match reader.u8()? {
        1 => MediaDeviceResult::Presence {
            microphone: reader.bool()?,
            camera: reader.bool()?,
        },
        2 => MediaDeviceResult::Error(MediaDeviceError::NotAllowed),
        3 => MediaDeviceResult::Error(MediaDeviceError::NotReadable),
        _ => return Err(ProtocolError::InvalidPayload("media-device update")),
    };
    reader.finish()?;
    let update = MediaDeviceUpdate {
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

    #[test]
    fn only_presence_bits_cross_the_wire() {
        let request = MediaDeviceRequest {
            document: DocumentId::new(7).unwrap(),
            request_id: 4,
            client: RequestClient {
                id: 0,
                opaque: false,
            },
        };
        assert_eq!(
            decode_request(&encode_request(&request).unwrap()).unwrap(),
            request
        );
        let update = MediaDeviceUpdate {
            document: request.document,
            request_id: request.request_id,
            result: MediaDeviceResult::Presence {
                microphone: true,
                camera: false,
            },
        };
        assert_eq!(
            decode_update(&encode_update(&update).unwrap()).unwrap(),
            update
        );
        let mut invalid = encode_update(&update).unwrap();
        invalid[17] = 2;
        assert!(decode_update(&invalid).is_err());
    }
}
