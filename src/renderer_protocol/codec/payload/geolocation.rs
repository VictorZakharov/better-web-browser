use crate::fetch::RequestClient;
use crate::renderer_protocol::wire::{WireReader, WireWriter};
use crate::renderer_protocol::{
    DocumentId, GeolocationAction, GeolocationErrorCode, GeolocationEvent, GeolocationPosition,
    GeolocationRequest, GeolocationUpdate, ProtocolError,
};

pub(super) fn encode_request(request: &GeolocationRequest) -> Result<Vec<u8>, ProtocolError> {
    request.validate()?;
    let mut writer = WireWriter::new();
    writer.u64(request.document.get());
    writer.u64(request.request_id);
    writer.u64(request.client.id);
    writer.bool(request.client.opaque);
    match request.action {
        GeolocationAction::Start {
            watch,
            high_accuracy,
            timeout_millis,
            maximum_age_millis,
        } => {
            writer.u8(1);
            writer.bool(watch);
            writer.bool(high_accuracy);
            writer.u64(timeout_millis);
            writer.u64(maximum_age_millis);
        }
        GeolocationAction::Clear => writer.u8(2),
    }
    Ok(writer.finish())
}

pub(super) fn decode_request(bytes: &[u8]) -> Result<GeolocationRequest, ProtocolError> {
    let mut reader = WireReader::new(bytes);
    let document = DocumentId::new(reader.u64()?)?;
    let request_id = reader.u64()?;
    let client = RequestClient {
        id: reader.u64()?,
        opaque: reader.bool()?,
    };
    let action = match reader.u8()? {
        1 => GeolocationAction::Start {
            watch: reader.bool()?,
            high_accuracy: reader.bool()?,
            timeout_millis: reader.u64()?,
            maximum_age_millis: reader.u64()?,
        },
        2 => GeolocationAction::Clear,
        _ => return Err(ProtocolError::InvalidPayload("geolocation action")),
    };
    reader.finish()?;
    let request = GeolocationRequest {
        document,
        request_id,
        client,
        action,
    };
    request.validate()?;
    Ok(request)
}

pub(super) fn encode_update(update: &GeolocationUpdate) -> Result<Vec<u8>, ProtocolError> {
    update.validate()?;
    let mut writer = WireWriter::new();
    writer.u64(update.document.get());
    writer.u64(update.request_id);
    writer.bool(update.terminal);
    match &update.event {
        GeolocationEvent::Position(position) => {
            writer.u8(1);
            for value in [position.latitude, position.longitude, position.accuracy] {
                writer.u64(value.to_bits());
            }
            for value in [
                position.altitude,
                position.altitude_accuracy,
                position.heading,
                position.speed,
            ] {
                writer.bool(value.is_some());
                if let Some(value) = value {
                    writer.u64(value.to_bits());
                }
            }
            writer.u64(position.timestamp_millis);
        }
        GeolocationEvent::Error { code, message } => {
            writer.u8(2);
            writer.u8(*code as u8);
            writer.string(message)?;
        }
    }
    Ok(writer.finish())
}

pub(super) fn decode_update(bytes: &[u8]) -> Result<GeolocationUpdate, ProtocolError> {
    let mut reader = WireReader::new(bytes);
    let document = DocumentId::new(reader.u64()?)?;
    let request_id = reader.u64()?;
    let terminal = reader.bool()?;
    let event = match reader.u8()? {
        1 => {
            let latitude = f64::from_bits(reader.u64()?);
            let longitude = f64::from_bits(reader.u64()?);
            let accuracy = f64::from_bits(reader.u64()?);
            let mut optional = || -> Result<Option<f64>, ProtocolError> {
                Ok(if reader.bool()? {
                    Some(f64::from_bits(reader.u64()?))
                } else {
                    None
                })
            };
            let altitude = optional()?;
            let altitude_accuracy = optional()?;
            let heading = optional()?;
            let speed = optional()?;
            GeolocationEvent::Position(GeolocationPosition {
                latitude,
                longitude,
                accuracy,
                altitude,
                altitude_accuracy,
                heading,
                speed,
                timestamp_millis: reader.u64()?,
            })
        }
        2 => GeolocationEvent::Error {
            code: match reader.u8()? {
                1 => GeolocationErrorCode::PermissionDenied,
                2 => GeolocationErrorCode::PositionUnavailable,
                3 => GeolocationErrorCode::Timeout,
                _ => return Err(ProtocolError::InvalidPayload("geolocation error code")),
            },
            message: reader.string(512)?,
        },
        _ => return Err(ProtocolError::InvalidPayload("geolocation update")),
    };
    reader.finish()?;
    let update = GeolocationUpdate {
        document,
        request_id,
        terminal,
        event,
    };
    update.validate()?;
    Ok(update)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_and_reject_malformed_coordinates() {
        let request = GeolocationRequest {
            document: DocumentId::new(7).unwrap(),
            request_id: 4,
            client: RequestClient {
                id: 2,
                opaque: false,
            },
            action: GeolocationAction::Start {
                watch: true,
                high_accuracy: false,
                timeout_millis: 5000,
                maximum_age_millis: 0,
            },
        };
        assert_eq!(
            decode_request(&encode_request(&request).unwrap()).unwrap(),
            request
        );
        let update = GeolocationUpdate {
            document: request.document,
            request_id: request.request_id,
            terminal: false,
            event: GeolocationEvent::Position(GeolocationPosition {
                latitude: 43.6,
                longitude: -79.4,
                accuracy: 10.0,
                altitude: None,
                altitude_accuracy: None,
                heading: None,
                speed: None,
                timestamp_millis: 123,
            }),
        };
        assert_eq!(
            decode_update(&encode_update(&update).unwrap()).unwrap(),
            update
        );
        let mut malformed = encode_update(&update).unwrap();
        malformed[18..26].copy_from_slice(&f64::NAN.to_bits().to_le_bytes());
        assert!(decode_update(&malformed).is_err());
    }
}
