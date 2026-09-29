use crate::fetch::RequestClient;
use crate::renderer_protocol::wire::{WireReader, WireWriter};
use crate::renderer_protocol::{
    DocumentId, MAX_CAPTURE_FRAME_BYTES, MediaCaptureAction, MediaCaptureError, MediaCaptureEvent,
    MediaCaptureFrame, MediaCaptureFrameKind, MediaCaptureRequest, MediaCaptureUpdate,
    ProtocolError,
};

pub(super) fn encode_request(request: &MediaCaptureRequest) -> Result<Vec<u8>, ProtocolError> {
    request.validate()?;
    let mut writer = WireWriter::new();
    writer.u64(request.document.get());
    writer.u64(request.request_id);
    writer.u64(request.client.id);
    writer.bool(request.client.opaque);
    match request.action {
        MediaCaptureAction::Start { camera, microphone } => {
            writer.u8(1);
            writer.bool(camera);
            writer.bool(microphone);
        }
        MediaCaptureAction::Stop { track_id } => {
            writer.u8(2);
            writer.u8(track_id);
        }
    }
    Ok(writer.finish())
}

pub(super) fn decode_request(bytes: &[u8]) -> Result<MediaCaptureRequest, ProtocolError> {
    let mut reader = WireReader::new(bytes);
    let document = DocumentId::new(reader.u64()?)?;
    let request_id = reader.u64()?;
    let client = RequestClient {
        id: reader.u64()?,
        opaque: reader.bool()?,
    };
    let action = match reader.u8()? {
        1 => MediaCaptureAction::Start {
            camera: reader.bool()?,
            microphone: reader.bool()?,
        },
        2 => MediaCaptureAction::Stop {
            track_id: reader.u8()?,
        },
        _ => return Err(ProtocolError::InvalidPayload("capture action")),
    };
    reader.finish()?;
    let request = MediaCaptureRequest {
        document,
        request_id,
        client,
        action,
    };
    request.validate()?;
    Ok(request)
}

pub(super) fn encode_update(update: &MediaCaptureUpdate) -> Result<Vec<u8>, ProtocolError> {
    update.validate()?;
    let mut writer = WireWriter::new();
    writer.u64(update.document.get());
    writer.u64(update.request_id);
    match update.event {
        MediaCaptureEvent::Started { camera, microphone } => {
            writer.u8(1);
            writer.bool(camera);
            writer.bool(microphone);
        }
        MediaCaptureEvent::Ended => writer.u8(2),
        MediaCaptureEvent::TrackEnded { track_id } => {
            writer.u8(7);
            writer.u8(track_id);
        }
        MediaCaptureEvent::Error(error) => writer.u8(match error {
            MediaCaptureError::NotAllowed => 3,
            MediaCaptureError::NotFound => 4,
            MediaCaptureError::NotReadable => 5,
            MediaCaptureError::Abort => 6,
        }),
    }
    Ok(writer.finish())
}

pub(super) fn decode_update(bytes: &[u8]) -> Result<MediaCaptureUpdate, ProtocolError> {
    let mut reader = WireReader::new(bytes);
    let document = DocumentId::new(reader.u64()?)?;
    let request_id = reader.u64()?;
    let event = match reader.u8()? {
        1 => MediaCaptureEvent::Started {
            camera: reader.bool()?,
            microphone: reader.bool()?,
        },
        2 => MediaCaptureEvent::Ended,
        7 => MediaCaptureEvent::TrackEnded {
            track_id: reader.u8()?,
        },
        3 => MediaCaptureEvent::Error(MediaCaptureError::NotAllowed),
        4 => MediaCaptureEvent::Error(MediaCaptureError::NotFound),
        5 => MediaCaptureEvent::Error(MediaCaptureError::NotReadable),
        6 => MediaCaptureEvent::Error(MediaCaptureError::Abort),
        _ => return Err(ProtocolError::InvalidPayload("capture update event")),
    };
    reader.finish()?;
    let update = MediaCaptureUpdate {
        document,
        request_id,
        event,
    };
    update.validate()?;
    Ok(update)
}

pub(super) fn encode_frame(frame: &MediaCaptureFrame) -> Result<Vec<u8>, ProtocolError> {
    frame.validate()?;
    let mut writer = WireWriter::new();
    writer.u64(frame.document.get());
    writer.u64(frame.request_id);
    writer.u64(frame.track_id);
    writer.u64(frame.sequence);
    writer.u64(frame.timestamp_100ns);
    writer.u8(match frame.kind {
        MediaCaptureFrameKind::VideoNv12 => 1,
        MediaCaptureFrameKind::AudioPcm16 => 2,
    });
    writer.u32(frame.width_or_rate);
    writer.u32(frame.height_or_frames);
    writer.u32(frame.stride_or_channels);
    writer.bytes(&frame.bytes)?;
    Ok(writer.finish())
}

pub(super) fn decode_frame(bytes: &[u8]) -> Result<MediaCaptureFrame, ProtocolError> {
    let mut reader = WireReader::new(bytes);
    let document = DocumentId::new(reader.u64()?)?;
    let request_id = reader.u64()?;
    let track_id = reader.u64()?;
    let sequence = reader.u64()?;
    let timestamp_100ns = reader.u64()?;
    let kind = match reader.u8()? {
        1 => MediaCaptureFrameKind::VideoNv12,
        2 => MediaCaptureFrameKind::AudioPcm16,
        _ => return Err(ProtocolError::InvalidPayload("capture frame kind")),
    };
    let width_or_rate = reader.u32()?;
    let height_or_frames = reader.u32()?;
    let stride_or_channels = reader.u32()?;
    let bytes = reader.bytes(MAX_CAPTURE_FRAME_BYTES)?;
    reader.finish()?;
    let frame = MediaCaptureFrame {
        document,
        request_id,
        track_id,
        sequence,
        timestamp_100ns,
        kind,
        width_or_rate,
        height_or_frames,
        stride_or_channels,
        bytes,
    };
    frame.validate()?;
    Ok(frame)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capture_messages_round_trip_and_reject_bad_shapes() {
        let document = DocumentId::new(3).unwrap();
        let request = MediaCaptureRequest {
            document,
            request_id: 9,
            client: RequestClient {
                id: 4,
                opaque: false,
            },
            action: MediaCaptureAction::Start {
                camera: true,
                microphone: false,
            },
        };
        assert_eq!(
            decode_request(&encode_request(&request).unwrap()).unwrap(),
            request
        );
        assert!(decode_request(&encode_request(&request).unwrap()[..20]).is_err());
        let update = MediaCaptureUpdate {
            document,
            request_id: 9,
            event: MediaCaptureEvent::Started {
                camera: true,
                microphone: false,
            },
        };
        assert_eq!(
            decode_update(&encode_update(&update).unwrap()).unwrap(),
            update
        );
        let frame = MediaCaptureFrame {
            document,
            request_id: 9,
            track_id: 2,
            sequence: 1,
            timestamp_100ns: 12,
            kind: MediaCaptureFrameKind::AudioPcm16,
            width_or_rate: 48_000,
            height_or_frames: 960,
            stride_or_channels: 1,
            bytes: vec![0; 1920],
        };
        assert_eq!(decode_frame(&encode_frame(&frame).unwrap()).unwrap(), frame);
        let mut malformed = encode_frame(&frame).unwrap();
        malformed[40] = 7;
        assert!(decode_frame(&malformed).is_err());
    }
}
