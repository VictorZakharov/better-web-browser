use super::MediaProtocolError;
use crate::limits::MAX_MEDIA_DURATION_100NS;

/// Extents accepted in this batch, not the presentation duration. A zero end means the
/// corresponding track was absent; independent audio/video appends never fabricate its range.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MediaBufferedExtent {
    pub video_start_100ns: i64,
    pub video_end_100ns: u64,
    pub audio_start_100ns: i64,
    pub audio_end_100ns: u64,
}

impl MediaBufferedExtent {
    pub fn validate(self) -> Result<(), MediaProtocolError> {
        if self.end_100ns() == 0 {
            return Err(MediaProtocolError::InvalidPayload("empty buffered extent"));
        }
        for (start, end) in [
            (self.video_start_100ns, self.video_end_100ns),
            (self.audio_start_100ns, self.audio_end_100ns),
        ] {
            if end > MAX_MEDIA_DURATION_100NS
                || (end == 0 && start != 0)
                || (end != 0 && (start < 0 || start as u64 >= end))
            {
                return Err(MediaProtocolError::InvalidPayload("buffered track extent"));
            }
        }
        Ok(())
    }

    pub fn end_100ns(self) -> u64 {
        self.video_end_100ns.max(self.audio_end_100ns)
    }

    pub fn seconds(self) -> [[f64; 2]; 2] {
        [
            [
                self.video_start_100ns as f64 / 10_000_000.0,
                self.video_end_100ns as f64 / 10_000_000.0,
            ],
            [
                self.audio_start_100ns as f64 / 10_000_000.0,
                self.audio_end_100ns as f64 / 10_000_000.0,
            ],
        ]
    }
}
