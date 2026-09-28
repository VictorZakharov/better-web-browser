use crate::renderer_protocol::wire::{WireReader, WireWriter};
use crate::renderer_protocol::{
    DocumentId, ProtocolError, SensorAction, SensorError, SensorEvent, SensorKind,
    SensorPermission, SensorReading, SensorRequest, SensorUpdate,
};

fn kind_tag(kind: SensorKind) -> u8 {
    match kind {
        SensorKind::Orientation => 1,
        SensorKind::Motion => 2,
        SensorKind::Accelerometer => 3,
        SensorKind::Gyroscope => 4,
        SensorKind::Magnetometer => 5,
        SensorKind::AbsoluteOrientation => 6,
        SensorKind::RelativeOrientation => 7,
        SensorKind::AmbientLight => 8,
    }
}

fn kind_from_tag(tag: u8) -> Result<SensorKind, ProtocolError> {
    match tag {
        1 => Ok(SensorKind::Orientation),
        2 => Ok(SensorKind::Motion),
        3 => Ok(SensorKind::Accelerometer),
        4 => Ok(SensorKind::Gyroscope),
        5 => Ok(SensorKind::Magnetometer),
        6 => Ok(SensorKind::AbsoluteOrientation),
        7 => Ok(SensorKind::RelativeOrientation),
        8 => Ok(SensorKind::AmbientLight),
        _ => Err(ProtocolError::InvalidPayload("sensor kind")),
    }
}

fn f64(writer: &mut WireWriter, value: f64) {
    writer.u64(value.to_bits());
}

fn read_f64(reader: &mut WireReader<'_>) -> Result<f64, ProtocolError> {
    Ok(f64::from_bits(reader.u64()?))
}

fn optional_f64(writer: &mut WireWriter, value: Option<f64>) {
    writer.bool(value.is_some());
    if let Some(value) = value {
        f64(writer, value);
    }
}

fn read_optional_f64(reader: &mut WireReader<'_>) -> Result<Option<f64>, ProtocolError> {
    reader.bool()?.then(|| read_f64(reader)).transpose()
}

fn optional_vector(writer: &mut WireWriter, value: Option<[f64; 3]>) {
    writer.bool(value.is_some());
    if let Some(value) = value {
        for coordinate in value {
            f64(writer, coordinate);
        }
    }
}

fn read_optional_vector(reader: &mut WireReader<'_>) -> Result<Option<[f64; 3]>, ProtocolError> {
    if !reader.bool()? {
        return Ok(None);
    }
    Ok(Some([
        read_f64(reader)?,
        read_f64(reader)?,
        read_f64(reader)?,
    ]))
}

pub(super) fn encode_request(request: &SensorRequest) -> Result<Vec<u8>, ProtocolError> {
    request.validate()?;
    let mut writer = WireWriter::new();
    writer.u64(request.document.get());
    writer.u64(request.request_id);
    writer.u64(request.client.id);
    writer.bool(request.client.opaque);
    writer.bool(request.user_activation);
    match request.action {
        SensorAction::RequestPermission { kind, absolute } => {
            writer.u8(1);
            writer.u8(kind_tag(kind));
            writer.bool(absolute);
        }
        SensorAction::Start { kind, frequency_hz } => {
            writer.u8(2);
            writer.u8(kind_tag(kind));
            optional_f64(&mut writer, frequency_hz);
        }
        SensorAction::Stop { kind } => {
            writer.u8(3);
            writer.u8(kind_tag(kind));
        }
    }
    Ok(writer.finish())
}

pub(super) fn decode_request(bytes: &[u8]) -> Result<SensorRequest, ProtocolError> {
    let mut reader = WireReader::new(bytes);
    let document = DocumentId::new(reader.u64()?)?;
    let request_id = reader.u64()?;
    let client = crate::fetch::RequestClient {
        id: reader.u64()?,
        opaque: reader.bool()?,
    };
    let user_activation = reader.bool()?;
    let action = match reader.u8()? {
        1 => SensorAction::RequestPermission {
            kind: kind_from_tag(reader.u8()?)?,
            absolute: reader.bool()?,
        },
        2 => SensorAction::Start {
            kind: kind_from_tag(reader.u8()?)?,
            frequency_hz: read_optional_f64(&mut reader)?,
        },
        3 => SensorAction::Stop {
            kind: kind_from_tag(reader.u8()?)?,
        },
        _ => return Err(ProtocolError::InvalidPayload("sensor action")),
    };
    reader.finish()?;
    let request = SensorRequest {
        document,
        request_id,
        client,
        user_activation,
        action,
    };
    request.validate()?;
    Ok(request)
}

pub(super) fn encode_update(update: &SensorUpdate) -> Result<Vec<u8>, ProtocolError> {
    update.validate()?;
    let mut writer = WireWriter::new();
    writer.u64(update.document.get());
    writer.u64(update.request_id);
    match &update.event {
        SensorEvent::Permission(SensorPermission::Granted) => writer.u8(1),
        SensorEvent::Permission(SensorPermission::Denied) => writer.u8(2),
        SensorEvent::Activated => writer.u8(3),
        SensorEvent::Error(error) => {
            writer.u8(4);
            writer.u8(match error {
                SensorError::NotAllowed => 1,
                SensorError::NotReadable => 2,
                SensorError::NotSupported => 3,
            });
        }
        SensorEvent::Reading(SensorReading::Orientation {
            alpha,
            beta,
            gamma,
            absolute,
        }) => {
            writer.u8(5);
            optional_f64(&mut writer, *alpha);
            optional_f64(&mut writer, *beta);
            optional_f64(&mut writer, *gamma);
            writer.bool(*absolute);
        }
        SensorEvent::Reading(SensorReading::Motion {
            acceleration,
            acceleration_including_gravity,
            rotation_rate,
            interval_ms,
        }) => {
            writer.u8(6);
            optional_vector(&mut writer, *acceleration);
            optional_vector(&mut writer, *acceleration_including_gravity);
            optional_vector(&mut writer, *rotation_rate);
            f64(&mut writer, *interval_ms);
        }
        SensorEvent::Reading(SensorReading::ThreeAxis {
            x,
            y,
            z,
            timestamp_ms,
        }) => {
            writer.u8(7);
            for value in [x, y, z, timestamp_ms] {
                f64(&mut writer, *value);
            }
        }
        SensorEvent::Reading(SensorReading::Quaternion {
            x,
            y,
            z,
            w,
            timestamp_ms,
        }) => {
            writer.u8(8);
            for value in [x, y, z, w, timestamp_ms] {
                f64(&mut writer, *value);
            }
        }
        SensorEvent::Reading(SensorReading::Illuminance {
            illuminance,
            timestamp_ms,
        }) => {
            writer.u8(9);
            f64(&mut writer, *illuminance);
            f64(&mut writer, *timestamp_ms);
        }
    }
    Ok(writer.finish())
}

pub(super) fn decode_update(bytes: &[u8]) -> Result<SensorUpdate, ProtocolError> {
    let mut reader = WireReader::new(bytes);
    let document = DocumentId::new(reader.u64()?)?;
    let request_id = reader.u64()?;
    let event = match reader.u8()? {
        1 => SensorEvent::Permission(SensorPermission::Granted),
        2 => SensorEvent::Permission(SensorPermission::Denied),
        3 => SensorEvent::Activated,
        4 => SensorEvent::Error(match reader.u8()? {
            1 => SensorError::NotAllowed,
            2 => SensorError::NotReadable,
            3 => SensorError::NotSupported,
            _ => return Err(ProtocolError::InvalidPayload("sensor error")),
        }),
        5 => SensorEvent::Reading(SensorReading::Orientation {
            alpha: read_optional_f64(&mut reader)?,
            beta: read_optional_f64(&mut reader)?,
            gamma: read_optional_f64(&mut reader)?,
            absolute: reader.bool()?,
        }),
        6 => SensorEvent::Reading(SensorReading::Motion {
            acceleration: read_optional_vector(&mut reader)?,
            acceleration_including_gravity: read_optional_vector(&mut reader)?,
            rotation_rate: read_optional_vector(&mut reader)?,
            interval_ms: read_f64(&mut reader)?,
        }),
        7 => SensorEvent::Reading(SensorReading::ThreeAxis {
            x: read_f64(&mut reader)?,
            y: read_f64(&mut reader)?,
            z: read_f64(&mut reader)?,
            timestamp_ms: read_f64(&mut reader)?,
        }),
        8 => SensorEvent::Reading(SensorReading::Quaternion {
            x: read_f64(&mut reader)?,
            y: read_f64(&mut reader)?,
            z: read_f64(&mut reader)?,
            w: read_f64(&mut reader)?,
            timestamp_ms: read_f64(&mut reader)?,
        }),
        9 => SensorEvent::Reading(SensorReading::Illuminance {
            illuminance: read_f64(&mut reader)?,
            timestamp_ms: read_f64(&mut reader)?,
        }),
        _ => return Err(ProtocolError::InvalidPayload("sensor event")),
    };
    reader.finish()?;
    let update = SensorUpdate {
        document,
        request_id,
        event,
    };
    update.validate()?;
    Ok(update)
}

#[cfg(test)]
#[path = "sensor/tests.rs"]
mod tests;
