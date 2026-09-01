use super::MediaProtocolError;
use crate::limits::MAX_MEDIA_FAILURE_BYTES;
use crate::media_protocol::{
    MediaLimits, MediaPixelFormat, MediaTestCommand, MediaVideoFrameMetadata,
};

pub(super) fn encode_limits(payload: &mut Vec<u8>, limits: MediaLimits) {
    vec_u32(payload, limits.max_control_payload);
    vec_u16(payload, limits.max_tracks);
    vec_u32(payload, limits.max_dimension);
    vec_u64(payload, limits.max_encoded_bytes);
    vec_u64(payload, limits.max_encoded_queue_bytes);
    vec_u64(payload, limits.max_decoded_frame_bytes);
    vec_u16(payload, limits.max_decoded_frames);
    vec_u16(payload, limits.max_decoder_candidates);
    vec_u32(payload, limits.probe_timeout_millis);
}

pub(super) fn decode_limits(cursor: &mut Cursor<'_>) -> Result<MediaLimits, MediaProtocolError> {
    Ok(MediaLimits {
        max_control_payload: cursor.u32()?,
        max_tracks: cursor.u16()?,
        max_dimension: cursor.u32()?,
        max_encoded_bytes: cursor.u64()?,
        max_encoded_queue_bytes: cursor.u64()?,
        max_decoded_frame_bytes: cursor.u64()?,
        max_decoded_frames: cursor.u16()?,
        max_decoder_candidates: cursor.u16()?,
        probe_timeout_millis: cursor.u32()?,
    })
}

pub(super) fn encode_frame_metadata(payload: &mut Vec<u8>, frame: MediaVideoFrameMetadata) {
    vec_u64(payload, frame.source_id);
    vec_u64(payload, frame.frame_id);
    vec_i64(payload, frame.timestamp_100ns);
    vec_u64(payload, frame.duration_100ns);
    vec_u32(payload, frame.width);
    vec_u32(payload, frame.height);
    vec_u32(payload, frame.stride);
    vec_u16(payload, frame.format.wire_code());
    vec_u64(payload, frame.data_length);
}

pub(super) fn decode_frame_metadata(
    cursor: &mut Cursor<'_>,
) -> Result<MediaVideoFrameMetadata, MediaProtocolError> {
    let frame = MediaVideoFrameMetadata {
        source_id: cursor.nonzero_u64("frame source")?,
        frame_id: cursor.nonzero_u64("frame generation")?,
        timestamp_100ns: cursor.i64()?,
        duration_100ns: cursor.u64()?,
        width: cursor.u32()?,
        height: cursor.u32()?,
        stride: cursor.u32()?,
        format: MediaPixelFormat::from_wire(cursor.u16()?)
            .map_err(|_| MediaProtocolError::InvalidPayload("frame pixel format"))?,
        data_length: cursor.u64()?,
    };
    frame
        .validate()
        .map_err(|_| MediaProtocolError::InvalidPayload("video frame metadata"))?;
    Ok(frame)
}

pub(super) fn encode_test(payload: &mut Vec<u8>, command: MediaTestCommand) {
    match command {
        MediaTestCommand::Crash => payload.push(1),
        MediaTestCommand::Hang => payload.push(2),
        MediaTestCommand::DelayResponse { millis } => {
            payload.push(3);
            vec_u16(payload, millis);
        }
        MediaTestCommand::WriteMalformedFrame => payload.push(4),
        MediaTestCommand::ProbeRestrictions { loopback_port } => {
            payload.push(5);
            vec_u16(payload, loopback_port);
        }
        MediaTestCommand::WriteMalformedDecodedFrame => payload.push(6),
        MediaTestCommand::WriteTruncatedDecodedFrame => payload.push(7),
        MediaTestCommand::WriteOversizedDecodedFrame => payload.push(8),
    }
}

pub(super) fn decode_test(cursor: &mut Cursor<'_>) -> Result<MediaTestCommand, MediaProtocolError> {
    match cursor.byte()? {
        1 => Ok(MediaTestCommand::Crash),
        2 => Ok(MediaTestCommand::Hang),
        3 => Ok(MediaTestCommand::DelayResponse {
            millis: cursor.u16()?,
        }),
        4 => Ok(MediaTestCommand::WriteMalformedFrame),
        5 => Ok(MediaTestCommand::ProbeRestrictions {
            loopback_port: cursor.u16()?,
        }),
        6 => Ok(MediaTestCommand::WriteMalformedDecodedFrame),
        7 => Ok(MediaTestCommand::WriteTruncatedDecodedFrame),
        8 => Ok(MediaTestCommand::WriteOversizedDecodedFrame),
        _ => Err(MediaProtocolError::InvalidPayload("test command")),
    }
}

pub(super) struct Cursor<'a> {
    remaining: &'a [u8],
}

impl<'a> Cursor<'a> {
    pub(super) fn new(bytes: &'a [u8]) -> Self {
        Self { remaining: bytes }
    }

    fn take(&mut self, count: usize) -> Result<&'a [u8], MediaProtocolError> {
        if self.remaining.len() < count {
            return Err(MediaProtocolError::InvalidPayload("truncated payload"));
        }
        let (value, remaining) = self.remaining.split_at(count);
        self.remaining = remaining;
        Ok(value)
    }

    pub(super) fn byte(&mut self) -> Result<u8, MediaProtocolError> {
        Ok(self.take(1)?[0])
    }

    pub(super) fn boolean(&mut self) -> Result<bool, MediaProtocolError> {
        match self.byte()? {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(MediaProtocolError::InvalidPayload("boolean")),
        }
    }

    pub(super) fn u16(&mut self) -> Result<u16, MediaProtocolError> {
        Ok(get_u16(self.take(2)?))
    }

    pub(super) fn u32(&mut self) -> Result<u32, MediaProtocolError> {
        Ok(get_u32(self.take(4)?))
    }

    pub(super) fn i32(&mut self) -> Result<i32, MediaProtocolError> {
        Ok(i32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }

    pub(super) fn i64(&mut self) -> Result<i64, MediaProtocolError> {
        Ok(i64::from_le_bytes(self.take(8)?.try_into().unwrap()))
    }

    pub(super) fn u64(&mut self) -> Result<u64, MediaProtocolError> {
        Ok(get_u64(self.take(8)?))
    }

    pub(super) fn nonzero_u64(&mut self, field: &'static str) -> Result<u64, MediaProtocolError> {
        let value = self.u64()?;
        require_nonzero(value, field)?;
        Ok(value)
    }

    pub(super) fn array<const N: usize>(&mut self) -> Result<[u8; N], MediaProtocolError> {
        Ok(self.take(N)?.try_into().unwrap())
    }

    pub(super) fn failure_string(&mut self) -> Result<String, MediaProtocolError> {
        let length = self.u16()? as usize;
        if length == 0 || length > MAX_MEDIA_FAILURE_BYTES {
            return Err(MediaProtocolError::InvalidPayload("media failure text"));
        }
        String::from_utf8(self.take(length)?.to_vec())
            .map_err(|_| MediaProtocolError::InvalidPayload("media failure UTF-8"))
    }

    pub(super) fn finish(self) -> Result<(), MediaProtocolError> {
        self.remaining
            .is_empty()
            .then_some(())
            .ok_or(MediaProtocolError::InvalidPayload("trailing bytes"))
    }
}

pub(super) fn require_nonzero(value: u64, field: &'static str) -> Result<(), MediaProtocolError> {
    (value != 0)
        .then_some(())
        .ok_or(MediaProtocolError::InvalidPayload(field))
}

pub(super) fn boolean(output: &mut Vec<u8>, value: bool) {
    output.push(u8::from(value));
}
pub(super) fn vec_u16(output: &mut Vec<u8>, value: u16) {
    output.extend_from_slice(&value.to_le_bytes());
}
pub(super) fn vec_u32(output: &mut Vec<u8>, value: u32) {
    output.extend_from_slice(&value.to_le_bytes());
}
pub(super) fn vec_i32(output: &mut Vec<u8>, value: i32) {
    output.extend_from_slice(&value.to_le_bytes());
}
pub(super) fn vec_i64(output: &mut Vec<u8>, value: i64) {
    output.extend_from_slice(&value.to_le_bytes());
}
pub(super) fn vec_u64(output: &mut Vec<u8>, value: u64) {
    output.extend_from_slice(&value.to_le_bytes());
}
pub(super) fn put_u16(output: &mut [u8], value: u16) {
    output.copy_from_slice(&value.to_le_bytes());
}
pub(super) fn put_u32(output: &mut [u8], value: u32) {
    output.copy_from_slice(&value.to_le_bytes());
}
pub(super) fn put_u64(output: &mut [u8], value: u64) {
    output.copy_from_slice(&value.to_le_bytes());
}
pub(super) fn get_u16(input: &[u8]) -> u16 {
    u16::from_le_bytes(input.try_into().unwrap())
}
pub(super) fn get_u32(input: &[u8]) -> u32 {
    u32::from_le_bytes(input.try_into().unwrap())
}
pub(super) fn get_u64(input: &[u8]) -> u64 {
    u64::from_le_bytes(input.try_into().unwrap())
}
