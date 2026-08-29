use super::{
    MediaFrameError, MediaFramePacket, MediaFrameTestFault, MediaPixelFormat,
    MediaVideoFrameMetadata,
};
use crate::limits::MAX_MEDIA_DATA_CHUNK_BYTES;
use crate::media_protocol::{MediaSessionId, Nonce};
use std::io::{Read, Write};

const MAGIC: [u8; 4] = *b"BRV1";
const HEADER_LENGTH: usize = 120;
const MAJOR: u16 = 1;
const MINOR: u16 = 0;
const FLAG_END: u16 = 1;

pub(crate) struct MediaFrameWriter<W> {
    inner: W,
    session: MediaSessionId,
    nonce: Nonce,
}

impl<W: Write> MediaFrameWriter<W> {
    pub(crate) fn new(inner: W, session: MediaSessionId, nonce: Nonce) -> Self {
        Self {
            inner,
            session,
            nonce,
        }
    }

    pub(crate) fn send_frame(
        &mut self,
        metadata: MediaVideoFrameMetadata,
        bytes: &[u8],
    ) -> Result<(), MediaFrameError> {
        metadata.validate()?;
        if bytes.len() as u64 != metadata.data_length {
            return Err(MediaFrameError::InvalidLength(bytes.len() as u64));
        }
        let mut offset = 0_u64;
        for chunk in bytes.chunks(MAX_MEDIA_DATA_CHUNK_BYTES) {
            self.write_frame(metadata, offset, 0, chunk)?;
            offset += chunk.len() as u64;
        }
        self.write_frame(metadata, offset, FLAG_END, &[])?;
        self.inner.flush()?;
        Ok(())
    }

    pub(crate) fn write_fault_for_test(
        &mut self,
        metadata: MediaVideoFrameMetadata,
        fault: MediaFrameTestFault,
    ) -> Result<(), MediaFrameError> {
        match fault {
            MediaFrameTestFault::Malformed => self.inner.write_all(&[0; HEADER_LENGTH])?,
            MediaFrameTestFault::Truncated => {
                let mut header = encode_header(self.session, self.nonce, metadata, 0, 0);
                put_u32(&mut header[12..16], 1);
                self.inner.write_all(&header)?;
            }
            MediaFrameTestFault::Oversized => {
                let mut invalid = metadata;
                invalid.data_length = crate::limits::MAX_MEDIA_DECODED_FRAME_BYTES as u64 + 1;
                let header = encode_header(self.session, self.nonce, invalid, 0, FLAG_END);
                self.inner.write_all(&header)?;
            }
        }
        self.inner.flush()?;
        Ok(())
    }

    fn write_frame(
        &mut self,
        metadata: MediaVideoFrameMetadata,
        offset: u64,
        flags: u16,
        payload: &[u8],
    ) -> Result<(), MediaFrameError> {
        if payload.len() > MAX_MEDIA_DATA_CHUNK_BYTES {
            return Err(MediaFrameError::ChunkTooLarge(payload.len() as u32));
        }
        let mut header = encode_header(self.session, self.nonce, metadata, offset, flags);
        put_u32(&mut header[12..16], payload.len() as u32);
        self.inner.write_all(&header)?;
        self.inner.write_all(payload)?;
        Ok(())
    }
}

pub(crate) struct MediaFrameReader<R> {
    inner: R,
    session: MediaSessionId,
    nonce: Nonce,
}

impl<R: Read> MediaFrameReader<R> {
    pub(crate) fn new(inner: R, session: MediaSessionId, nonce: Nonce) -> Self {
        Self {
            inner,
            session,
            nonce,
        }
    }

    pub(crate) fn read_frame(
        &mut self,
        expected_source: u64,
        expected_frame: u64,
    ) -> Result<MediaFramePacket, MediaFrameError> {
        let mut result = Vec::new();
        let mut expected_metadata = None;
        loop {
            let mut header = [0_u8; HEADER_LENGTH];
            self.inner.read_exact(&mut header)?;
            let (metadata, offset, flags, chunk_length) = self.decode_header(
                &header,
                expected_source,
                expected_frame,
                result.len() as u64,
            )?;
            metadata.validate()?;
            if let Some(expected) = expected_metadata {
                if metadata != expected {
                    return Err(MediaFrameError::MetadataChanged);
                }
            } else {
                result = Vec::with_capacity(
                    usize::try_from(metadata.data_length)
                        .map_err(|_| MediaFrameError::InvalidLength(metadata.data_length))?,
                );
                expected_metadata = Some(metadata);
            }
            if flags == FLAG_END {
                if chunk_length != 0 || offset != metadata.data_length {
                    return Err(MediaFrameError::PrematureEnd);
                }
                return Ok(MediaFramePacket {
                    metadata,
                    nv12: result,
                });
            }
            if chunk_length == 0 {
                return Err(MediaFrameError::MissingPayload);
            }
            if chunk_length as usize > MAX_MEDIA_DATA_CHUNK_BYTES {
                return Err(MediaFrameError::ChunkTooLarge(chunk_length));
            }
            let remaining = metadata.data_length.saturating_sub(result.len() as u64);
            if u64::from(chunk_length) > remaining {
                return Err(MediaFrameError::InvalidLength(metadata.data_length));
            }
            let start = result.len();
            result.resize(start + chunk_length as usize, 0);
            self.inner.read_exact(&mut result[start..])?;
        }
    }

    fn decode_header(
        &self,
        header: &[u8; HEADER_LENGTH],
        expected_source: u64,
        expected_frame: u64,
        expected_offset: u64,
    ) -> Result<(MediaVideoFrameMetadata, u64, u16, u32), MediaFrameError> {
        if header[..4] != MAGIC {
            return Err(MediaFrameError::InvalidMagic);
        }
        let major = get_u16(&header[4..6]);
        let minor = get_u16(&header[6..8]);
        if major != MAJOR || minor > MINOR {
            return Err(MediaFrameError::IncompatibleVersion { major, minor });
        }
        let flags = get_u16(&header[8..10]);
        if !matches!(flags, 0 | FLAG_END) || get_u16(&header[10..12]) != 0 {
            return Err(MediaFrameError::InvalidFlags(flags));
        }
        let actual_session = get_u64(&header[16..24]);
        if actual_session != self.session.get() {
            return Err(MediaFrameError::WrongSession {
                expected: self.session.get(),
                actual: actual_session,
            });
        }
        if header[88..120] != self.nonce.as_bytes()[..] {
            return Err(MediaFrameError::WrongNonce);
        }
        let source = get_u64(&header[24..32]);
        if source != expected_source {
            return Err(MediaFrameError::WrongSource {
                expected: expected_source,
                actual: source,
            });
        }
        let frame = get_u64(&header[32..40]);
        if frame != expected_frame {
            return Err(MediaFrameError::WrongFrame {
                expected: expected_frame,
                actual: frame,
            });
        }
        let offset = get_u64(&header[40..48]);
        if offset != expected_offset {
            return Err(MediaFrameError::WrongOffset {
                expected: expected_offset,
                actual: offset,
            });
        }
        Ok((
            MediaVideoFrameMetadata {
                source_id: source,
                frame_id: frame,
                timestamp_100ns: get_i64(&header[48..56]),
                duration_100ns: get_u64(&header[56..64]),
                width: get_u32(&header[64..68]),
                height: get_u32(&header[68..72]),
                stride: get_u32(&header[72..76]),
                format: MediaPixelFormat::from_wire(get_u16(&header[76..78]))?,
                data_length: get_u64(&header[80..88]),
            },
            offset,
            flags,
            get_u32(&header[12..16]),
        ))
    }
}

fn encode_header(
    session: MediaSessionId,
    nonce: Nonce,
    metadata: MediaVideoFrameMetadata,
    offset: u64,
    flags: u16,
) -> [u8; HEADER_LENGTH] {
    let mut header = [0_u8; HEADER_LENGTH];
    header[..4].copy_from_slice(&MAGIC);
    put_u16(&mut header[4..6], MAJOR);
    put_u16(&mut header[6..8], MINOR);
    put_u16(&mut header[8..10], flags);
    put_u64(&mut header[16..24], session.get());
    put_u64(&mut header[24..32], metadata.source_id);
    put_u64(&mut header[32..40], metadata.frame_id);
    put_u64(&mut header[40..48], offset);
    put_i64(&mut header[48..56], metadata.timestamp_100ns);
    put_u64(&mut header[56..64], metadata.duration_100ns);
    put_u32(&mut header[64..68], metadata.width);
    put_u32(&mut header[68..72], metadata.height);
    put_u32(&mut header[72..76], metadata.stride);
    put_u16(&mut header[76..78], metadata.format.wire_code());
    put_u64(&mut header[80..88], metadata.data_length);
    header[88..120].copy_from_slice(nonce.as_bytes());
    header
}

fn put_u16(output: &mut [u8], value: u16) {
    output.copy_from_slice(&value.to_le_bytes());
}
fn put_u32(output: &mut [u8], value: u32) {
    output.copy_from_slice(&value.to_le_bytes());
}
fn put_u64(output: &mut [u8], value: u64) {
    output.copy_from_slice(&value.to_le_bytes());
}
fn put_i64(output: &mut [u8], value: i64) {
    output.copy_from_slice(&value.to_le_bytes());
}
fn get_u16(input: &[u8]) -> u16 {
    u16::from_le_bytes(input.try_into().unwrap())
}
fn get_u32(input: &[u8]) -> u32 {
    u32::from_le_bytes(input.try_into().unwrap())
}
fn get_u64(input: &[u8]) -> u64 {
    u64::from_le_bytes(input.try_into().unwrap())
}
fn get_i64(input: &[u8]) -> i64 {
    i64::from_le_bytes(input.try_into().unwrap())
}

#[cfg(test)]
pub(super) const TEST_HEADER_LENGTH: usize = HEADER_LENGTH;
