use crate::renderer_protocol::wire::{WireReader, WireWriter};
use crate::renderer_protocol::{
    DocumentId, PermissionName, PermissionRequest, PermissionState, PermissionUpdate, ProtocolError,
};

fn write_name(writer: &mut WireWriter, name: PermissionName) {
    writer.u8(match name {
        PermissionName::Notifications => 1,
        PermissionName::Geolocation => 2,
        PermissionName::ClipboardRead => 8,
        PermissionName::ClipboardWrite => 9,
        PermissionName::Accelerometer => 4,
        PermissionName::Gyroscope => 5,
        PermissionName::Magnetometer => 6,
        PermissionName::AmbientLightSensor => 7,
    });
}

fn read_name(reader: &mut WireReader<'_>) -> Result<PermissionName, ProtocolError> {
    Ok(match reader.u8()? {
        1 => PermissionName::Notifications,
        2 => PermissionName::Geolocation,
        8 => PermissionName::ClipboardRead,
        9 => PermissionName::ClipboardWrite,
        4 => PermissionName::Accelerometer,
        5 => PermissionName::Gyroscope,
        6 => PermissionName::Magnetometer,
        7 => PermissionName::AmbientLightSensor,
        _ => return Err(ProtocolError::InvalidPayload("permission name")),
    })
}

pub(super) fn encode_request(request: &PermissionRequest) -> Result<Vec<u8>, ProtocolError> {
    request.validate()?;
    let mut writer = WireWriter::new();
    writer.u64(request.document.get());
    writer.u64(request.request_id);
    writer.u64(request.client.id);
    writer.bool(request.client.opaque);
    writer.bool(request.embedded);
    write_name(&mut writer, request.name);
    Ok(writer.finish())
}

pub(super) fn decode_request(bytes: &[u8]) -> Result<PermissionRequest, ProtocolError> {
    let mut reader = WireReader::new(bytes);
    let request = PermissionRequest {
        document: DocumentId::new(reader.u64()?)?,
        request_id: reader.u64()?,
        client: crate::fetch::RequestClient {
            id: reader.u64()?,
            opaque: reader.bool()?,
        },
        embedded: reader.bool()?,
        name: read_name(&mut reader)?,
    };
    reader.finish()?;
    request.validate()?;
    Ok(request)
}

pub(super) fn encode_update(update: &PermissionUpdate) -> Result<Vec<u8>, ProtocolError> {
    update.validate()?;
    let mut writer = WireWriter::new();
    writer.u64(update.document.get());
    writer.u64(update.request_id);
    write_name(&mut writer, update.name);
    writer.u8(match update.state {
        PermissionState::Prompt => 1,
        PermissionState::Granted => 2,
        PermissionState::Denied => 3,
    });
    writer.bool(update.rejected);
    Ok(writer.finish())
}

pub(super) fn decode_update(bytes: &[u8]) -> Result<PermissionUpdate, ProtocolError> {
    let mut reader = WireReader::new(bytes);
    let document = DocumentId::new(reader.u64()?)?;
    let request_id = reader.u64()?;
    let name = read_name(&mut reader)?;
    let state = match reader.u8()? {
        1 => PermissionState::Prompt,
        2 => PermissionState::Granted,
        3 => PermissionState::Denied,
        _ => return Err(ProtocolError::InvalidPayload("permission state")),
    };
    let rejected = reader.bool()?;
    reader.finish()?;
    let update = PermissionUpdate {
        document,
        request_id,
        name,
        state,
        rejected,
    };
    update.validate()?;
    Ok(update)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn query_and_change_round_trip_with_bounded_tags() {
        let request = PermissionRequest {
            document: DocumentId::new(8).unwrap(),
            request_id: 4,
            client: crate::fetch::RequestClient {
                id: 2,
                opaque: false,
            },
            embedded: true,
            name: PermissionName::Geolocation,
        };
        assert_eq!(
            decode_request(&encode_request(&request).unwrap()).unwrap(),
            request
        );
        let update = PermissionUpdate {
            document: request.document,
            request_id: 4,
            name: request.name,
            state: PermissionState::Denied,
            rejected: false,
        };
        assert_eq!(
            decode_update(&encode_update(&update).unwrap()).unwrap(),
            update
        );
        let mut corrupt = encode_request(&request).unwrap();
        corrupt[26] = 255;
        assert!(decode_request(&corrupt).is_err());
    }
}
