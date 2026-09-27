use crate::limits::MAX_MEDIA_CONTROL_PAYLOAD;
use crate::media_protocol::MediaProtocolError;

/// Interleaved signed 16-bit PCM sent to the contained output worker.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GraphPcmFormat {
    pub sample_rate: u32,
    pub channels: u16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum GraphPcmStatus {
    Accepted = 0,
    Backpressure = 1,
    Rejected = 2,
}

impl GraphPcmStatus {
    pub(crate) fn from_wire(value: u8) -> Result<Self, MediaProtocolError> {
        match value {
            0 => Ok(Self::Accepted),
            1 => Ok(Self::Backpressure),
            2 => Ok(Self::Rejected),
            _ => Err(MediaProtocolError::InvalidPayload("graph PCM status")),
        }
    }
}

// Seven 128-frame Web Audio quanta fit beside the 32-byte command metadata under
// the existing 4 KiB media-control payload ceiling, including stereo PCM16.
pub const MAX_GRAPH_PCM_FRAMES: usize = 896;
const GRAPH_PCM_CONTROL_METADATA_BYTES: usize = 3 * 8 + 4 + 2 + 2;
const _: () = assert!(
    GRAPH_PCM_CONTROL_METADATA_BYTES + MAX_GRAPH_PCM_FRAMES * 2 * 2 <= MAX_MEDIA_CONTROL_PAYLOAD
);

impl GraphPcmFormat {
    pub fn validate(self, bytes: usize) -> Result<(), MediaProtocolError> {
        // The output voice accepts one or two channels; wider Web Audio graphs must mix down
        // before this transport. XAudio2 accepts these source rates and converts for the device.
        if !(8_000..=192_000).contains(&self.sample_rate) || !(1..=2).contains(&self.channels) {
            return Err(MediaProtocolError::InvalidPayload("graph PCM format"));
        }
        let block_align = usize::from(self.channels) * 2;
        if bytes == 0
            || !bytes.is_multiple_of(block_align)
            || bytes / block_align > MAX_GRAPH_PCM_FRAMES
        {
            return Err(MediaProtocolError::InvalidPayload("graph PCM byte count"));
        }
        Ok(())
    }
}
