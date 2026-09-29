//! Incremental FLAC encoding for one live microphone track per recorder.
//!
//! RFC 9639 permits STREAMINFO to leave frame sizes, total samples, and MD5
//! unknown. We write that header once, then serialize each encoded frame as it
//! arrives. No `flacenc::Stream` retains the recording in native memory.

use flacenc::bitsink::ByteSink;
use flacenc::component::{BitRepr, Frame, FrameOffset, Stream, StreamInfo};
use flacenc::config;
use flacenc::error::{Verified, Verify};
use flacenc::source::{Fill, FrameBuf};
use std::collections::HashMap;

const MAX_RECORDERS: usize = 8;
const MAX_INPUT_FRAMES: usize = 960;
const MIN_FRAME_SAMPLES: usize = 16;
// flacenc's reusable FrameBuf has a 32-sample capacity minimum, but accepts a
// shorter filled final frame; the encoded block still carries the true length.
const MIN_BUFFER_CAPACITY: usize = 32;
const MAX_SAMPLE_OFFSET: u64 = (1 << 36) - 1;

#[derive(Default)]
pub(super) struct MediaRecorders {
    next_id: u32,
    sessions: HashMap<u32, Session>,
}

struct Session {
    config: Verified<config::Encoder>,
    info: Option<StreamInfo>,
    format: Option<(usize, usize)>,
    pending: Vec<u8>,
    sample_offset: u64,
}

impl MediaRecorders {
    pub(super) fn open(&mut self) -> Result<u32, &'static str> {
        if self.sessions.len() >= MAX_RECORDERS {
            return Err("Too many active audio recorders");
        }
        let mut config = config::Encoder::default();
        config.block_size = MAX_INPUT_FRAMES;
        // Fixed prediction and constant coding provide useful compression with
        // bounded per-packet work on the script callback path.
        config.subframe_coding.use_lpc = false;
        let config = config
            .into_verified()
            .map_err(|_| "Invalid FLAC encoder configuration")?;
        let id = self
            .next_id
            .checked_add(1)
            .ok_or("Recorder ID limit reached")?;
        self.next_id = id;
        self.sessions.insert(
            id,
            Session {
                config,
                info: None,
                format: None,
                pending: Vec::with_capacity(MAX_INPUT_FRAMES * 4),
                sample_offset: 0,
            },
        );
        Ok(id)
    }

    pub(super) fn append(
        &mut self,
        id: u32,
        sample_rate: usize,
        channels: usize,
        pcm: &[u8],
    ) -> Result<Vec<u8>, &'static str> {
        let session = self.sessions.get_mut(&id).ok_or("Unknown audio recorder")?;
        session.append(sample_rate, channels, pcm)
    }

    pub(super) fn finish(&mut self, id: u32) -> Result<Vec<u8>, &'static str> {
        let mut session = self.sessions.remove(&id).ok_or("Unknown audio recorder")?;
        session.finish()
    }

    pub(super) fn cancel(&mut self, id: u32) {
        self.sessions.remove(&id);
    }
}

impl Session {
    fn append(
        &mut self,
        sample_rate: usize,
        channels: usize,
        pcm: &[u8],
    ) -> Result<Vec<u8>, &'static str> {
        if !(8_000..=48_000).contains(&sample_rate)
            || !(1..=2).contains(&channels)
            || pcm.is_empty()
            || pcm.len() > MAX_INPUT_FRAMES * channels * 2
            || !pcm.len().is_multiple_of(channels * 2)
        {
            return Err("Invalid capture PCM format");
        }
        if self
            .format
            .is_some_and(|format| format != (sample_rate, channels))
        {
            return Err("Capture PCM format changed during recording");
        }

        let mut output = Vec::new();
        if self.info.is_none() {
            let mut info = StreamInfo::new(sample_rate, channels, 16)
                .map_err(|_| "Invalid FLAC stream format")?;
            // Variable block sizes allow dataavailable to divide a stream at
            // packet boundaries. The minimum is conservative; the last frame
            // may contain fewer than 16 samples, per RFC 9639 section 8.2.
            info.set_block_sizes(MIN_FRAME_SAMPLES, MAX_INPUT_FRAMES)
                .map_err(|_| "Invalid FLAC block sizes")?;
            info.set_frame_sizes(0, 0)
                .map_err(|_| "Invalid FLAC frame sizes")?;
            let mut sink = ByteSink::new();
            Stream::with_stream_info(info.clone())
                .write(&mut sink)
                .map_err(|_| "Could not write FLAC header")?;
            output.extend_from_slice(sink.as_slice());
            self.info = Some(info);
            self.format = Some((sample_rate, channels));
        }

        self.pending.extend_from_slice(pcm);
        let stride = channels * 2;
        while self.pending.len() / stride >= MIN_FRAME_SAMPLES {
            let frames = (self.pending.len() / stride).min(MAX_INPUT_FRAMES);
            output.extend(self.encode_frame(frames)?);
        }
        Ok(output)
    }

    fn finish(&mut self) -> Result<Vec<u8>, &'static str> {
        let Some((_, channels)) = self.format else {
            return Ok(Vec::new());
        };
        let frames = self.pending.len() / (channels * 2);
        if frames == 0 {
            return Ok(Vec::new());
        }
        self.encode_frame(frames)
    }

    fn encode_frame(&mut self, frames: usize) -> Result<Vec<u8>, &'static str> {
        let info = self.info.as_ref().ok_or("Missing FLAC stream header")?;
        let channels = info.channels();
        let byte_count = frames * channels * 2;
        let next_offset = self
            .sample_offset
            .checked_add(frames as u64)
            .filter(|offset| *offset <= MAX_SAMPLE_OFFSET)
            .ok_or("FLAC recording duration limit reached")?;
        let mut buffer = FrameBuf::with_size(channels, frames.max(MIN_BUFFER_CAPACITY))
            .map_err(|_| "Invalid FLAC frame size")?;
        buffer
            .fill_le_bytes(&self.pending[..byte_count], 2)
            .map_err(|_| "Invalid capture PCM samples")?;
        let frame = flacenc::encode_fixed_size_frame(&self.config, &buffer, 0, info)
            .map_err(|_| "FLAC frame encoding failed")?;
        let (mut header, subframes) = frame.into_parts();
        header.set_frame_offset(FrameOffset::StartSample(self.sample_offset));
        let frame = Frame::new(header, subframes.into_iter())
            .map_err(|_| "FLAC frame construction failed")?;
        let mut sink = ByteSink::new();
        frame
            .write(&mut sink)
            .map_err(|_| "Could not write FLAC frame")?;
        self.pending.drain(..byte_count);
        self.sample_offset = next_offset;
        Ok(sink.into_inner())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn pcm(samples: &[i16]) -> Vec<u8> {
        samples
            .iter()
            .flat_map(|sample| sample.to_le_bytes())
            .collect()
    }

    fn decoded(bytes: Vec<u8>) -> Vec<i32> {
        claxon::FlacReader::new(Cursor::new(bytes))
            .expect("FLAC header and frame must decode")
            .samples()
            .map(|sample| sample.expect("lossless PCM sample"))
            .collect()
    }

    #[test]
    fn chunks_form_one_lossless_variable_block_stream() {
        let mut recorders = MediaRecorders::default();
        let id = recorders.open().unwrap();
        let first: Vec<i16> = (0..160).map(|n| (n * 127 - 8000) as i16).collect();
        let second: Vec<i16> = (0..160).map(|n| (8000 - n * 83) as i16).collect();
        let mut output = recorders.append(id, 8_000, 1, &pcm(&first)).unwrap();
        assert_eq!(&output[..4], b"fLaC");
        output.extend(recorders.append(id, 8_000, 1, &pcm(&second)).unwrap());
        output.extend(recorders.finish(id).unwrap());
        let expected = first
            .into_iter()
            .chain(second)
            .map(i32::from)
            .collect::<Vec<_>>();
        assert_eq!(decoded(output), expected);
    }

    #[test]
    fn last_frame_preserves_one_to_fifteen_samples_without_padding() {
        for tail in 1..16 {
            let mut recorders = MediaRecorders::default();
            let id = recorders.open().unwrap();
            let first = vec![4096_i16; 32];
            let last = (0..tail).map(|n| -3000 + n as i16).collect::<Vec<_>>();
            let mut output = recorders.append(id, 8_000, 1, &pcm(&first)).unwrap();
            output.extend(recorders.append(id, 8_000, 1, &pcm(&last)).unwrap());
            output.extend(recorders.finish(id).unwrap());
            let expected = first
                .into_iter()
                .chain(last)
                .map(i32::from)
                .collect::<Vec<_>>();
            assert_eq!(decoded(output), expected, "tail length {tail}");
        }
    }

    #[test]
    fn a_single_short_capture_packet_is_decodable() {
        let mut recorders = MediaRecorders::default();
        let id = recorders.open().unwrap();
        let mut output = recorders
            .append(id, 48_000, 2, &pcm(&[1, -2, 3, -4, 5, -6]))
            .unwrap();
        output.extend(recorders.finish(id).unwrap());
        assert_eq!(decoded(output), [1, -2, 3, -4, 5, -6]);
    }

    #[test]
    fn capture_input_and_session_count_are_bounded() {
        let mut recorders = MediaRecorders::default();
        let ids = (0..MAX_RECORDERS)
            .map(|_| recorders.open().unwrap())
            .collect::<Vec<_>>();
        assert!(recorders.open().is_err());
        let id = ids[0];
        assert!(
            recorders
                .append(id, 8_000, 1, &vec![0; (MAX_INPUT_FRAMES + 1) * 2])
                .is_err()
        );
        recorders.append(id, 8_000, 1, &pcm(&[1, 2, 3])).unwrap();
        assert!(recorders.append(id, 48_000, 1, &pcm(&[4])).is_err());
        recorders.cancel(id);
        assert!(recorders.append(id, 8_000, 1, &pcm(&[5])).is_err());
        assert!(recorders.open().is_ok());
    }
}
