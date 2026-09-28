use super::*;
use crate::renderer_protocol::{
    BrowserMessage, FrameReader, FrameWriter, RendererMessage, RendererSessionId,
};

#[test]
fn round_trip_permission_and_readings() {
    let request = SensorRequest {
        document: DocumentId::new(2).unwrap(),
        request_id: 42,
        client: crate::fetch::RequestClient {
            id: 3,
            opaque: false,
        },
        user_activation: true,
        action: SensorAction::RequestPermission {
            kind: SensorKind::Orientation,
            absolute: true,
        },
    };
    assert_eq!(
        decode_request(&encode_request(&request).unwrap()).unwrap(),
        request
    );
    let update = SensorUpdate {
        document: request.document,
        request_id: request.request_id,
        event: SensorEvent::Reading(SensorReading::Motion {
            acceleration: None,
            acceleration_including_gravity: Some([1.0, 2.0, 3.0]),
            rotation_rate: Some([4.0, 5.0, 6.0]),
            interval_ms: 16.0,
        }),
    };
    assert_eq!(
        decode_update(&encode_update(&update).unwrap()).unwrap(),
        update
    );
    // FrameReader applies a separate directional tag allowlist. Exercise that
    // boundary as well as the payload codec to prevent valid new messages
    // being rejected before decode (as a hidden renderer initially revealed).
    let session = RendererSessionId::new(7).unwrap();
    let mut renderer_writer = FrameWriter::new(Vec::new(), session);
    renderer_writer
        .send_renderer(&RendererMessage::SensorRequest(request.clone()))
        .unwrap();
    let bytes = renderer_writer.into_inner();
    assert_eq!(
        FrameReader::new(bytes.as_slice(), session)
            .read_renderer()
            .unwrap(),
        RendererMessage::SensorRequest(request)
    );
    let mut browser_writer = FrameWriter::new(Vec::new(), session);
    browser_writer
        .send_browser(&BrowserMessage::SensorUpdate(update.clone()))
        .unwrap();
    let bytes = browser_writer.into_inner();
    assert_eq!(
        FrameReader::new(bytes.as_slice(), session)
            .read_browser()
            .unwrap(),
        BrowserMessage::SensorUpdate(update.clone())
    );
    let mut truncated = encode_update(&update).unwrap();
    truncated.pop();
    assert!(decode_update(&truncated).is_err());
    let invalid = SensorUpdate {
        event: SensorEvent::Reading(SensorReading::ThreeAxis {
            x: f64::NAN,
            y: 0.0,
            z: 0.0,
            timestamp_ms: 0.0,
        }),
        ..update
    };
    assert!(encode_update(&invalid).is_err());
}

#[test]
fn magnetic_field_and_absolute_quaternion_keep_units_and_components() {
    let document = DocumentId::new(17).unwrap();
    let client = crate::fetch::RequestClient {
        id: 4,
        opaque: false,
    };
    for (request_id, kind) in [
        (5, SensorKind::Magnetometer),
        (6, SensorKind::AbsoluteOrientation),
        (7, SensorKind::RelativeOrientation),
        (8, SensorKind::AmbientLight),
        (9, SensorKind::LinearAcceleration),
        (10, SensorKind::Gravity),
        (11, SensorKind::OrientationAbsoluteLegacy),
    ] {
        let request = SensorRequest {
            document,
            request_id,
            client,
            user_activation: false,
            action: SensorAction::Start {
                kind,
                frequency_hz: Some(20.0),
            },
        };
        assert_eq!(
            decode_request(&encode_request(&request).unwrap()).unwrap(),
            request
        );
    }
    let magnetic = SensorUpdate {
        document,
        request_id: 5,
        event: SensorEvent::Reading(SensorReading::ThreeAxis {
            // Magnetometer values are already in microtesla on the wire.
            x: 48.25,
            y: -7.5,
            z: 19.75,
            timestamp_ms: 100.0,
        }),
    };
    assert_eq!(
        decode_update(&encode_update(&magnetic).unwrap()).unwrap(),
        magnetic
    );
    let orientation = SensorUpdate {
        document,
        request_id: 6,
        event: SensorEvent::Reading(SensorReading::Quaternion {
            x: 0.0,
            y: 0.0,
            z: std::f64::consts::FRAC_1_SQRT_2,
            w: std::f64::consts::FRAC_1_SQRT_2,
            timestamp_ms: 101.0,
        }),
    };
    assert_eq!(
        decode_update(&encode_update(&orientation).unwrap()).unwrap(),
        orientation
    );
    let illuminance = SensorUpdate {
        document,
        request_id: 8,
        event: SensorEvent::Reading(SensorReading::Illuminance {
            // Quantization is performed by the browser before transport.
            illuminance: 150.0,
            timestamp_ms: 102.0,
        }),
    };
    assert_eq!(
        decode_update(&encode_update(&illuminance).unwrap()).unwrap(),
        illuminance
    );
    assert!(
        encode_update(&SensorUpdate {
            event: SensorEvent::Reading(SensorReading::Illuminance {
                illuminance: -1.0,
                timestamp_ms: 102.0,
            }),
            ..illuminance
        })
        .is_err()
    );
    let invalid = SensorUpdate {
        event: SensorEvent::Reading(SensorReading::Quaternion {
            x: 0.0,
            y: 0.0,
            z: 0.0,
            w: 0.0,
            timestamp_ms: 101.0,
        }),
        ..orientation
    };
    assert!(encode_update(&invalid).is_err());
}
