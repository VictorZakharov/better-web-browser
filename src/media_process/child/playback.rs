use super::super::backend;
use super::audio::AudioPlayback;
use crate::media_data_protocol::{MediaDataReader, MediaSourceId};
use crate::media_frame_protocol::{
    MediaFrameWriter as DecodedFrameWriter, MediaPixelFormat, MediaVideoFrameMetadata,
};
use crate::media_protocol::{MediaFrameWriter, MediaLimits, WorkerMediaMessage};
use std::fs::File;

pub(super) struct Playback {
    last_source_id: u64,
    last_frame_id: u64,
    pending: Option<(MediaVideoFrameMetadata, Vec<u8>)>,
    active: Option<(u64, backend::VideoDecoder)>,
    audio: Option<(u64, AudioPlayback)>,
    test_mode: bool,
}

impl Playback {
    pub(super) fn new(test_mode: bool) -> Self {
        Self {
            last_source_id: 0,
            last_frame_id: 0,
            pending: None,
            active: None,
            audio: None,
            test_mode,
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn decode_source(
        &mut self,
        request_id: u64,
        source_id: u64,
        frame_id: u64,
        encoded_length: u64,
        data_reader: &mut MediaDataReader<File>,
        frame_writer: &mut DecodedFrameWriter<File>,
        writer: &mut MediaFrameWriter<File>,
        limits: MediaLimits,
    ) -> Result<(), String> {
        if self.pending.is_some() {
            return Err("media worker received a decode before acknowledging its frame".into());
        }
        let expected_source_id = self
            .last_source_id
            .checked_add(1)
            .ok_or_else(|| "media source generation exhausted".to_string())?;
        if source_id != expected_source_id {
            return Err(format!(
                "stale media source generation {source_id}; expected {expected_source_id}"
            ));
        }
        let expected_frame_id = self
            .last_frame_id
            .checked_add(1)
            .ok_or_else(|| "media frame generation exhausted".to_string())?;
        if frame_id != expected_frame_id {
            return Err(format!(
                "stale media frame generation {frame_id}; expected {expected_frame_id}"
            ));
        }
        // Complete-source admission is intentionally restricted to the resident budget until the
        // streaming network service can feed this worker without privileged byte ownership.
        if encoded_length > limits.max_encoded_queue_bytes {
            return Err("declared media source exceeds resident worker limits".into());
        }
        let source = MediaSourceId::new(source_id).map_err(|error| error.to_string())?;
        let bytes = data_reader
            .read_source(source, encoded_length)
            .map_err(|error| format!("read encoded media source: {error}"))?;
        self.last_source_id = source_id;
        self.last_frame_id = frame_id;
        let decoded = backend::decode(&bytes, limits)?;
        self.install_decoded(
            request_id,
            source_id,
            source_id,
            frame_id,
            bytes,
            decoded,
            frame_writer,
            writer,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn decode_tracks(
        &mut self,
        request_id: u64,
        video_source_id: u64,
        audio_source_id: u64,
        frame_id: u64,
        video_length: u64,
        audio_length: u64,
        data_reader: &mut MediaDataReader<File>,
        frame_writer: &mut DecodedFrameWriter<File>,
        writer: &mut MediaFrameWriter<File>,
        limits: MediaLimits,
    ) -> Result<(), String> {
        if self.pending.is_some() {
            return Err("media worker received a decode before acknowledging its frame".into());
        }
        let expected_video_id = self
            .last_source_id
            .checked_add(1)
            .ok_or_else(|| "media source generation exhausted".to_string())?;
        let expected_audio_id = expected_video_id
            .checked_add(1)
            .ok_or_else(|| "media source generation exhausted".to_string())?;
        if video_source_id != expected_video_id || audio_source_id != expected_audio_id {
            return Err(format!(
                "stale adaptive source generation {video_source_id}/{audio_source_id}; expected {expected_video_id}/{expected_audio_id}"
            ));
        }
        let expected_frame_id = self
            .last_frame_id
            .checked_add(1)
            .ok_or_else(|| "media frame generation exhausted".to_string())?;
        if frame_id != expected_frame_id {
            return Err(format!(
                "stale media frame generation {frame_id}; expected {expected_frame_id}"
            ));
        }
        let encoded_length = video_length
            .checked_add(audio_length)
            .ok_or_else(|| "adaptive media length overflowed".to_string())?;
        if encoded_length > limits.max_encoded_queue_bytes {
            return Err("declared adaptive source exceeds resident worker limits".into());
        }
        let video_source =
            MediaSourceId::new(video_source_id).map_err(|error| error.to_string())?;
        let audio_source =
            MediaSourceId::new(audio_source_id).map_err(|error| error.to_string())?;
        let video_bytes = data_reader
            .read_source(video_source, video_length)
            .map_err(|error| format!("read encoded video source: {error}"))?;
        let audio_bytes = data_reader
            .read_source(audio_source, audio_length)
            .map_err(|error| format!("read encoded audio source: {error}"))?;
        self.last_source_id = audio_source_id;
        self.last_frame_id = frame_id;
        let decoded = backend::decode_tracks(&video_bytes, &audio_bytes, limits)?;
        self.install_decoded(
            request_id,
            video_source_id,
            audio_source_id,
            frame_id,
            audio_bytes,
            decoded,
            frame_writer,
            writer,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn install_decoded(
        &mut self,
        request_id: u64,
        source_id: u64,
        last_transfer_source_id: u64,
        frame_id: u64,
        audio_bytes: Vec<u8>,
        decoded: backend::DecodedMedia,
        frame_writer: &mut DecodedFrameWriter<File>,
        writer: &mut MediaFrameWriter<File>,
    ) -> Result<(), String> {
        let backend::DecodedMedia {
            report,
            mut playback,
        } = decoded;
        let audio = AudioPlayback::spawn(source_id, audio_bytes, report, self.test_mode)?;
        let video = playback
            .next_frame()?
            .ok_or_else(|| "decoded video stream did not produce a frame".to_string())?;
        let frame = MediaVideoFrameMetadata {
            source_id,
            frame_id,
            timestamp_100ns: video.timestamp_100ns,
            duration_100ns: video.duration_100ns,
            width: report.video_width,
            height: report.video_height,
            stride: video.stride,
            format: MediaPixelFormat::Nv12,
            data_length: video.bytes.len() as u64,
        };
        validate_and_write(frame_writer, frame, &video.bytes)?;
        self.last_source_id = last_transfer_source_id;
        self.last_frame_id = frame_id;
        self.pending = Some((frame, video.bytes));
        self.active = Some((source_id, playback));
        self.audio = Some((source_id, audio));
        writer
            .send_worker(&WorkerMediaMessage::Decoded {
                request_id,
                report,
                frame,
            })
            .map_err(|error| error.to_string())
    }

    pub(super) fn acknowledge(
        &mut self,
        source_id: u64,
        frame_id: u64,
        writer: &mut MediaFrameWriter<File>,
    ) -> Result<(), String> {
        let Some((frame, _bytes)) = self.pending.as_ref() else {
            return Err(
                "media worker received a frame acknowledgement with no pending frame".into(),
            );
        };
        if source_id != frame.source_id || frame_id != frame.frame_id {
            return Err(format!(
                "stale media frame acknowledgement {source_id}/{frame_id}; expected {}/{}",
                frame.source_id, frame.frame_id
            ));
        }
        self.pending.take();
        writer
            .send_worker(&WorkerMediaMessage::FrameAcknowledged {
                source_id,
                frame_id,
            })
            .map_err(|error| error.to_string())
    }

    pub(super) fn request_frame(
        &mut self,
        source_id: u64,
        frame_id: u64,
        frame_writer: &mut DecodedFrameWriter<File>,
        writer: &mut MediaFrameWriter<File>,
    ) -> Result<(), String> {
        if self.pending.is_some() {
            return Err(
                "media worker received a frame request before acknowledging its frame".into(),
            );
        }
        let Some((active_source_id, playback)) = self.active.as_mut() else {
            return Err("media worker received a frame request with no active source".into());
        };
        if source_id != *active_source_id {
            return Err(format!(
                "stale media frame request for source {source_id}; expected {active_source_id}"
            ));
        }
        let expected_frame_id = self
            .last_frame_id
            .checked_add(1)
            .ok_or_else(|| "media frame generation exhausted".to_string())?;
        if frame_id != expected_frame_id {
            return Err(format!(
                "stale media frame generation {frame_id}; expected {expected_frame_id}"
            ));
        }
        let Some(video) = playback.next_frame()? else {
            return writer
                .send_worker(&WorkerMediaMessage::EndOfStream { source_id })
                .map_err(|error| error.to_string());
        };
        let (width, height) = playback.dimensions();
        let frame = MediaVideoFrameMetadata {
            source_id,
            frame_id,
            timestamp_100ns: video.timestamp_100ns,
            duration_100ns: video.duration_100ns,
            width,
            height,
            stride: video.stride,
            format: MediaPixelFormat::Nv12,
            data_length: video.bytes.len() as u64,
        };
        validate_and_write(frame_writer, frame, &video.bytes)?;
        self.last_frame_id = frame_id;
        self.pending = Some((frame, video.bytes));
        writer
            .send_worker(&WorkerMediaMessage::FrameReady { frame })
            .map_err(|error| error.to_string())
    }

    pub(super) fn set_playback(
        &self,
        source_id: u64,
        playing: bool,
        volume_millis: u16,
    ) -> Result<crate::media_protocol::MediaPlaybackState, String> {
        let Some((active_source_id, audio)) = self.audio.as_ref() else {
            return Err("media worker received playback control with no active source".into());
        };
        if source_id != *active_source_id {
            return Err(format!(
                "stale playback control for source {source_id}; expected {active_source_id}"
            ));
        }
        audio.set_playback(playing, volume_millis)
    }

    pub(super) fn playback_state(
        &self,
        source_id: u64,
    ) -> Result<crate::media_protocol::MediaPlaybackState, String> {
        let Some((active_source_id, audio)) = self.audio.as_ref() else {
            return Err("media worker received playback query with no active source".into());
        };
        if source_id != *active_source_id {
            return Err(format!(
                "stale playback query for source {source_id}; expected {active_source_id}"
            ));
        }
        audio.state()
    }

    pub(super) fn seek(
        &mut self,
        source_id: u64,
        position_100ns: u64,
    ) -> Result<crate::media_protocol::MediaPlaybackState, String> {
        if self.pending.is_some() {
            return Err("media worker received a seek before acknowledging its frame".into());
        }
        let Some((active_source_id, video)) = self.active.as_mut() else {
            return Err("media worker received a seek with no active source".into());
        };
        if source_id != *active_source_id {
            return Err(format!(
                "stale playback seek for source {source_id}; expected {active_source_id}"
            ));
        }
        video.seek(position_100ns)?;
        let Some((audio_source_id, audio)) = self.audio.as_ref() else {
            return Err("media worker received a seek with no audio clock".into());
        };
        if source_id != *audio_source_id {
            return Err("media worker audio/video source identity disagreed".into());
        }
        audio.seek(position_100ns)
    }
}

fn validate_and_write(
    writer: &mut DecodedFrameWriter<File>,
    frame: MediaVideoFrameMetadata,
    bytes: &[u8],
) -> Result<(), String> {
    frame
        .validate()
        .map_err(|error| format!("validate decoded video frame: {error}"))?;
    writer
        .send_frame(frame, bytes)
        .map_err(|error| format!("write decoded video frame: {error}"))
}
