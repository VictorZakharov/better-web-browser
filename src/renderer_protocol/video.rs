//! Bounded video pixels tied to an already committed document presentation.

use super::wire::{WireReader, WireWriter};
use super::{DocumentId, ProtocolError};
use crate::limits::{MAX_MEDIA_DECODED_FRAME_BYTES, MAX_MEDIA_DIMENSION};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VideoFrameIdentity {
    pub document: DocumentId,
    pub revision: u64,
    pub frame: u64,
    pub node: u128,
    pub width: u32,
    pub height: u32,
}

impl VideoFrameIdentity {
    pub fn byte_length(&self) -> Result<usize, ProtocolError> {
        let length = u64::from(self.width)
            .checked_mul(u64::from(self.height))
            .and_then(|pixels| pixels.checked_mul(4))
            .unwrap_or(u64::MAX);
        if self.revision == 0
            || self.frame == 0
            || self.node == 0
            || self.width == 0
            || self.height == 0
            || self.width > MAX_MEDIA_DIMENSION
            || self.height > MAX_MEDIA_DIMENSION
            || length > MAX_MEDIA_DECODED_FRAME_BYTES as u64
        {
            return Err(ProtocolError::InvalidPayload(
                "video frame identity or dimensions",
            ));
        }
        Ok(length as usize)
    }

    pub fn image_key(&self) -> String {
        format!("breeze-internal:media-frame:{:032x}", self.node)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VideoFrameChunk {
    pub identity: VideoFrameIdentity,
    pub offset: u32,
    pub bytes: Vec<u8>,
}

impl VideoFrameChunk {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        let length = self.identity.byte_length()?;
        if self.bytes.is_empty()
            || self.bytes.len() > 1024 * 1024
            || (self.offset as usize)
                .checked_add(self.bytes.len())
                .is_none_or(|end| end > length)
        {
            return Err(ProtocolError::InvalidPayload("video frame chunk extent"));
        }
        Ok(())
    }

    pub(super) fn encode(&self) -> Result<Vec<u8>, ProtocolError> {
        self.validate()?;
        let mut writer = WireWriter::new();
        writer.u64(self.identity.document.get());
        writer.u64(self.identity.revision);
        writer.u64(self.identity.frame);
        writer.u128(self.identity.node);
        writer.u32(self.identity.width);
        writer.u32(self.identity.height);
        writer.u32(self.offset);
        writer.bytes(&self.bytes)?;
        Ok(writer.finish())
    }

    pub(super) fn decode(bytes: &[u8]) -> Result<Self, ProtocolError> {
        let mut reader = WireReader::new(bytes);
        let identity = VideoFrameIdentity {
            document: DocumentId::new(reader.u64()?)?,
            revision: reader.u64()?,
            frame: reader.u64()?,
            node: reader.u128()?,
            width: reader.u32()?,
            height: reader.u32()?,
        };
        identity.byte_length()?;
        let result = Self {
            identity,
            offset: reader.u32()?,
            bytes: reader.bytes(1024 * 1024)?,
        };
        reader.finish()?;
        result.validate()?;
        Ok(result)
    }
}

#[derive(Clone, Debug)]
pub struct VideoFrameUpdate {
    pub identity: VideoFrameIdentity,
    pub pixels: std::sync::Arc<[u8]>,
}

/// One in-flight image; identity and contiguous offsets are checked on every chunk.
#[derive(Default)]
pub struct VideoFrameAssembler {
    pending: Option<(VideoFrameIdentity, Vec<u8>)>,
}

impl VideoFrameAssembler {
    pub fn push(
        &mut self,
        chunk: VideoFrameChunk,
    ) -> Result<Option<VideoFrameUpdate>, ProtocolError> {
        chunk.validate()?;
        if self.pending.is_none() && chunk.offset == 0 {
            self.pending = Some((
                chunk.identity.clone(),
                Vec::with_capacity(chunk.identity.byte_length()?),
            ));
        }
        let (identity, pixels) = self
            .pending
            .as_mut()
            .ok_or(ProtocolError::InvalidPayload("video chunk without start"))?;
        if *identity != chunk.identity || pixels.len() != chunk.offset as usize {
            return Err(ProtocolError::InvalidPayload(
                "video chunk identity or offset",
            ));
        }
        pixels.extend_from_slice(&chunk.bytes);
        if pixels.len() != identity.byte_length()? {
            return Ok(None);
        }
        let (identity, pixels) = self.pending.take().expect("validated video assembly");
        Ok(Some(VideoFrameUpdate {
            identity,
            pixels: pixels.into(),
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chunk() -> VideoFrameChunk {
        VideoFrameChunk {
            identity: VideoFrameIdentity {
                document: DocumentId::new(1).unwrap(),
                revision: 1,
                frame: 1,
                node: 1,
                width: 2,
                height: 2,
            },
            offset: 0,
            bytes: vec![255; 8],
        }
    }

    #[test]
    fn frame_is_emitted_only_after_contiguous_complete_pixels() {
        let first = chunk();
        assert_eq!(
            VideoFrameChunk::decode(&first.encode().unwrap()).unwrap(),
            first
        );
        let mut assembler = VideoFrameAssembler::default();
        assert!(assembler.push(first.clone()).unwrap().is_none());
        let mut last = first;
        last.offset = 8;
        let result = assembler.push(last).unwrap().unwrap();
        assert_eq!(result.pixels.len(), 16);
    }

    #[test]
    fn stale_identity_and_overlapping_offsets_are_rejected() {
        for mismatch in 0..3 {
            let mut assembler = VideoFrameAssembler::default();
            assembler.push(chunk()).unwrap();
            let mut last = chunk();
            last.offset = 8;
            match mismatch {
                0 => last.identity.revision += 1,
                1 => last.identity.frame += 1,
                _ => last.offset = 0,
            }
            assert!(assembler.push(last).is_err());
        }
    }

    #[test]
    fn invalid_dimensions_and_oversized_chunks_fail_before_assembly() {
        let mut value = chunk();
        value.identity.width = u32::MAX;
        value.identity.height = u32::MAX;
        assert!(value.encode().is_err());
        value = chunk();
        value.offset = u32::MAX;
        assert!(value.encode().is_err());
    }
}
