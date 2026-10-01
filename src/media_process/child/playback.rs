mod append;
mod controls;
mod video_clock;
use self::video_clock::VideoClock;
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
    video_clock: Option<VideoClock>,
    encoded_bytes: u64,
    silent_audio: bool,
    // Last field drops after both native video and the joined audio thread.
    foundation: Option<backend::MediaFoundation>,
}

impl Playback {
    pub(super) fn new(silent_audio: bool) -> Self {
        Self {
            last_source_id: 0,
            last_frame_id: 0,
            pending: None,
            active: None,
            audio: None,
            video_clock: None,
            encoded_bytes: 0,
            silent_audio,
            foundation: None,
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
            playback,
            foundation,
        } = decoded;
        let audio = if report.audio_codec == crate::media_protocol::MediaCodecFamily::None {
            None
        } else {
            Some(AudioPlayback::spawn(
                source_id,
                audio_bytes,
                report.into(),
                self.silent_audio,
            )?)
        };
        let (playback, frame) = if let Some(mut playback) = playback {
            let video = playback
                .next_frame()?
                .ok_or_else(|| "decoded video stream did not produce a frame".to_string())?;
            let frame = video_frame_metadata(source_id, frame_id, &video);
            validate_and_write(frame_writer, frame, &video.bytes)?;
            self.pending = Some((frame, video.bytes));
            (Some(playback), Some(frame))
        } else {
            (None, None)
        };
        self.last_source_id = last_transfer_source_id;
        self.last_frame_id = frame_id;
        self.active = playback.map(|playback| (source_id, playback));
        self.audio = audio.map(|audio| (source_id, audio));
        self.foundation = foundation;
        self.video_clock = (report.audio_codec == crate::media_protocol::MediaCodecFamily::None)
            .then(|| VideoClock::new(source_id, report.duration_100ns));
        self.encoded_bytes = report.encoded_bytes;
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
        frame_writer: &mut DecodedFrameWriter<impl std::io::Write>,
        writer: &mut MediaFrameWriter<impl std::io::Write>,
    ) -> Result<(), String> {
        if self.pending.is_some() {
            return Err(
                "media worker received a frame request before acknowledging its frame".into(),
            );
        }
        if self.active.as_ref().map(|(id, _)| *id) != Some(source_id)
            && self.audio.as_ref().map(|(id, _)| *id) != Some(source_id)
        {
            return Err(format!("stale media frame request for source {source_id}"));
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
        self.last_frame_id = frame_id;
        let Some((_, playback)) = self.active.as_mut() else {
            return writer
                .send_worker(&WorkerMediaMessage::EndOfStream { source_id })
                .map_err(|error| error.to_string());
        };
        let decoded = playback.next_frame();
        let video = match decoded {
            Ok(video) => video,
            Err(error) => {
                // Codec rejection is scoped to this source, not the control protocol.
                // Retire its audio as well and allow a later source to replace it.
                self.active = None;
                self.audio = None;
                self.video_clock = None;
                return writer
                    .send_worker(&WorkerMediaMessage::DecodeFailed {
                        request_id: frame_id,
                        error: super::bounded_media_failure(error),
                    })
                    .map_err(|error| error.to_string());
            }
        };
        let Some(video) = video else {
            // End-of-buffer still answers this request. The client has consumed its identity
            // and may poll again or append more media before another frame is available.
            return writer
                .send_worker(&WorkerMediaMessage::EndOfStream { source_id })
                .map_err(|error| error.to_string());
        };
        let frame = video_frame_metadata(source_id, frame_id, &video);
        validate_and_write(frame_writer, frame, &video.bytes)?;
        self.pending = Some((frame, video.bytes));
        writer
            .send_worker(&WorkerMediaMessage::FrameReady { frame })
            .map_err(|error| error.to_string())
    }
}

fn video_frame_metadata(
    source_id: u64,
    frame_id: u64,
    video: &backend::DecodedVideoSample,
) -> MediaVideoFrameMetadata {
    MediaVideoFrameMetadata {
        source_id,
        frame_id,
        timestamp_100ns: video.timestamp_100ns,
        duration_100ns: video.duration_100ns,
        width: video.width,
        height: video.height,
        stride: video.stride,
        format: MediaPixelFormat::Nv12,
        data_length: video.bytes.len() as u64,
    }
}

fn validate_and_write(
    writer: &mut DecodedFrameWriter<impl std::io::Write>,
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

#[cfg(test)]
mod tests;
