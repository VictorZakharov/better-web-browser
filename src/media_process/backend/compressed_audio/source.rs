use super::{Kind, frames_to_100ns, mp3_has_declared_frames};
use crate::iso_bmff_audio::{FrameWindow, ordinary_audio_edit};
use crate::limits::{
    MAX_MEDIA_DECODED_AUDIO_SAMPLE_BYTES, MAX_MEDIA_DECODED_SAMPLES, MAX_MEDIA_DECODED_SOURCE_BYTES,
};
use std::io::Cursor;
use std::sync::Arc;
use symphonia::core::codecs::audio::AudioDecoder as SymphoniaAudioDecoder;
use symphonia::core::codecs::audio::AudioDecoderOptions;
use symphonia::core::codecs::audio::well_known::{CODEC_ID_AAC, CODEC_ID_MP3};
use symphonia::core::common::Limit;
use symphonia::core::formats::probe::Hint;
use symphonia::core::formats::well_known::{FORMAT_ID_ISOMP4, FORMAT_ID_MP3};
use symphonia::core::formats::{FormatOptions, FormatReader};
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;

const MAX_COMPRESSED_PACKET_BYTES: usize = 256 * 1024;
// MP3 encoder delay and AAC priming/padding may produce extra PCM around a
// container's playable-frame count. A deficit larger than two MP3 packets is
// still evidence of a truncated complete-file source.
const DECLARED_FRAME_SLACK: u64 = 2_304;

pub(super) struct Stream {
    format: Box<dyn FormatReader>,
    decoder: Box<dyn SymphoniaAudioDecoder>,
    track_id: u32,
    declared_frames: Option<u64>,
    edit: Option<FrameWindow>,
    kind: Kind,
    packet_count: u32,
    raw_frames: u64,
    pub(super) samples: u32,
    pub(super) frames: u64,
    pub(super) decoded_bytes: u64,
    pub(super) sample_rate: Option<u32>,
    pub(super) channels: Option<u16>,
    pub(super) last_timestamp_100ns: i64,
    planar: Vec<Vec<i16>>,
    ended: bool,
}

impl Stream {
    pub(super) fn open(source: Arc<[u8]>, kind: Kind) -> Result<Self, String> {
        let edit = if kind == Kind::AacM4a {
            ordinary_audio_edit(&source)?
        } else {
            None
        };
        let mut hint = Hint::new();
        hint.with_extension(kind.extension());
        let media =
            MediaSourceStream::new(Box::new(Cursor::new(source.clone())), Default::default());
        let metadata = MetadataOptions::default()
            .limit_tag_bytes(Limit::Maximum(64 * 1024))
            .limit_visual_bytes(Limit::Maximum(0));
        let format = symphonia::default::get_probe()
            .probe(&hint, media, FormatOptions::default(), metadata)
            .map_err(|error| format!("open {} audio: {error}", kind.extension()))?;
        let (expected_format, expected_codec) = match kind {
            Kind::Mp3 => (FORMAT_ID_MP3, CODEC_ID_MP3),
            Kind::AacM4a => (FORMAT_ID_ISOMP4, CODEC_ID_AAC),
        };
        if format.format_info().format != expected_format || format.tracks().len() != 1 {
            return Err("compressed audio container is not a single-track audio source".into());
        }
        let track = &format.tracks()[0];
        let params = track
            .codec_params
            .as_ref()
            .and_then(|params| params.audio())
            .ok_or("compressed audio source has no audio track")?;
        if params.codec != expected_codec {
            return Err("compressed audio track codec is unsupported".into());
        }
        let edit_rate = if edit.is_some() {
            Some(
                params
                    .sample_rate
                    .ok_or("ISO BMFF edit has no audio sample rate")?,
            )
        } else {
            None
        };
        let edit = edit
            .map(|value| value.frame_window(edit_rate.unwrap()))
            .transpose()?;
        // MP3 packet trims from LAME/Xing stay enabled. For edited AAC, the
        // edit list itself defines the raw-media sample window, so applying a
        // second decoder trim would shift both endpoints.
        let decoder = symphonia::default::get_codecs()
            .make_audio_decoder(
                params,
                &AudioDecoderOptions::default()
                    .gapless(edit.is_none())
                    .verify(true),
            )
            .map_err(|error| format!("initialize {} decoder: {error}", kind.extension()))?;
        let declared_frames = if kind == Kind::Mp3 && !mp3_has_declared_frames(&source) {
            None
        } else {
            track.num_frames
        };
        let track_id = track.id;
        Ok(Self {
            format,
            decoder,
            track_id,
            declared_frames,
            edit,
            kind,
            packet_count: 0,
            raw_frames: 0,
            samples: 0,
            frames: 0,
            decoded_bytes: 0,
            sample_rate: edit_rate,
            channels: None,
            last_timestamp_100ns: 0,
            planar: Vec::new(),
            ended: false,
        })
    }

    pub(super) fn next_pcm(&mut self) -> Result<Option<Vec<u8>>, String> {
        if self.ended {
            return Ok(None);
        }
        loop {
            let Some(packet) = self
                .format
                .next_packet()
                .map_err(|error| format!("read {} packet: {error}", self.kind.extension()))?
            else {
                self.finish()?;
                return Ok(None);
            };
            if packet.track_id != self.track_id {
                continue;
            }
            self.packet_count = self
                .packet_count
                .checked_add(1)
                .ok_or_else(|| "compressed audio packet count overflow".to_string())?;
            if self.packet_count as usize > MAX_MEDIA_DECODED_SAMPLES
                || packet.data.len() > MAX_COMPRESSED_PACKET_BYTES
            {
                return Err("compressed audio packet exceeds worker limits".into());
            }
            let decoded = self
                .decoder
                .decode(&packet)
                .map_err(|error| format!("decode {} packet: {error}", self.kind.extension()))?;
            let rate = decoded.spec().rate();
            let channels = u16::try_from(decoded.num_planes())
                .map_err(|_| "compressed audio channel count overflow".to_string())?;
            if !(8_000..=192_000).contains(&rate)
                || !(1..=2).contains(&channels)
                || self.sample_rate.is_some_and(|previous| previous != rate)
                || self.channels.is_some_and(|previous| previous != channels)
            {
                return Err("compressed audio sample rate or channels changed".into());
            }
            self.sample_rate = Some(rate);
            self.channels = Some(channels);
            let frames = decoded.frames();
            let packet_bytes = frames
                .checked_mul(usize::from(channels) * 2)
                .ok_or_else(|| "compressed audio PCM packet length overflow".to_string())?;
            if packet_bytes > MAX_MEDIA_DECODED_AUDIO_SAMPLE_BYTES {
                return Err("compressed audio PCM packet exceeds worker limit".into());
            }
            if frames == 0 {
                continue;
            }
            let next_raw_frames = self
                .raw_frames
                .checked_add(frames as u64)
                .ok_or_else(|| "compressed audio frame count overflow".to_string())?;
            frames_to_100ns(next_raw_frames, rate)?;
            let raw_bytes = next_raw_frames
                .checked_mul(u64::from(channels) * 2)
                .ok_or("compressed audio raw PCM length overflow")?;
            if raw_bytes > MAX_MEDIA_DECODED_SOURCE_BYTES {
                return Err("compressed audio raw PCM exceeds worker limit".into());
            }
            let raw_start = self.raw_frames;
            self.raw_frames = next_raw_frames;
            let (keep_start, keep_end) = match self.edit {
                Some(window) => (raw_start.max(window.start), next_raw_frames.min(window.end)),
                None => (raw_start, next_raw_frames),
            };
            if keep_start >= keep_end {
                continue;
            }
            let first = usize::try_from(keep_start - raw_start)
                .map_err(|_| "compressed audio edit offset overflow")?;
            let last = usize::try_from(keep_end - raw_start)
                .map_err(|_| "compressed audio edit offset overflow")?;
            let kept_frames = keep_end - keep_start;
            let next_frames = self
                .frames
                .checked_add(kept_frames)
                .ok_or("compressed audio presented frame count overflow")?;
            frames_to_100ns(next_frames, rate)?;
            let kept_bytes = kept_frames
                .checked_mul(u64::from(channels) * 2)
                .ok_or("compressed audio presented PCM length overflow")?;
            let next_bytes = self
                .decoded_bytes
                .checked_add(kept_bytes)
                .ok_or_else(|| "compressed audio decoded length overflow".to_string())?;
            if next_bytes > MAX_MEDIA_DECODED_SOURCE_BYTES {
                return Err("compressed audio decoded bytes exceed worker limit".into());
            }
            decoded.copy_to_vecs_planar::<i16>(&mut self.planar);
            if self.planar.len() != usize::from(channels)
                || self.planar.iter().any(|plane| plane.len() != frames)
            {
                return Err("compressed audio PCM plane alignment is invalid".into());
            }
            let mut pcm = Vec::with_capacity(kept_bytes as usize);
            for frame in first..last {
                for plane in &self.planar {
                    pcm.extend_from_slice(&plane[frame].to_le_bytes());
                }
            }
            self.last_timestamp_100ns = i64::try_from(frames_to_100ns(self.frames, rate)?)
                .map_err(|_| "compressed audio timestamp overflow")?;
            self.frames = next_frames;
            self.decoded_bytes = next_bytes;
            self.samples += 1;
            return Ok(Some(pcm));
        }
    }

    fn finish(&mut self) -> Result<(), String> {
        if self.decoder.finalize().verify_ok == Some(false) {
            return Err("compressed audio checksum verification failed".into());
        }
        if self.edit.is_some_and(|window| {
            self.raw_frames < window.end || self.frames != window.end - window.start
        }) {
            return Err("compressed audio source ended before its edited presentation".into());
        }
        if self
            .declared_frames
            .is_some_and(|declared| self.raw_frames.saturating_add(DECLARED_FRAME_SLACK) < declared)
        {
            return Err("compressed audio source ended before declared duration".into());
        }
        self.ended = true;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::Engine;

    #[test]
    fn m4a_edit_selects_exact_pcm_window_from_decoded_media_timeline() {
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(
                include_str!("../../../../tests/fixtures/media/test-0.4s-tone.m4a.base64")
                    .lines()
                    .collect::<String>(),
            )
            .unwrap();
        let source: Arc<[u8]> = Arc::from(bytes);
        let mut raw = Stream::open(source.clone(), Kind::AacM4a).unwrap();
        raw.edit = None;
        let mut raw_pcm = Vec::new();
        while let Some(pcm) = raw.next_pcm().unwrap() {
            raw_pcm.extend(pcm);
        }
        let mut edited = Stream::open(source, Kind::AacM4a).unwrap();
        let mut presented = Vec::new();
        while let Some(pcm) = edited.next_pcm().unwrap() {
            presented.extend(pcm);
        }
        assert!(raw_pcm.len() >= 18_664 * 2);
        assert_eq!(presented, raw_pcm[1_024 * 2..18_664 * 2]);
        assert_eq!(edited.frames, 17_640);
    }
}
