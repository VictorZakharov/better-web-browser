//! Renderer-side client for a browser-launched contained media worker.
mod append;
mod retirement;

use super::broker::DecodedMediaFrame;
use crate::media_data_protocol::{MediaDataWriter, MediaSourceId};
use crate::media_frame_protocol::{MediaFramePacket, nv12_to_bgra};
use crate::media_protocol::{
    BrowserMediaMessage, MediaDecodeReport, MediaFrameWriter, MediaLimits, MediaPlaybackState,
    MediaProtocolError, MediaSessionId, Nonce, WorkerMediaMessage,
};
use std::fs::File;
use std::sync::mpsc::Receiver;
use std::thread::JoinHandle;
use std::time::Duration;

mod support;
#[cfg(test)]
mod tests;

use support::{
    checked_next, receive_from, receive_from_with_progress, spawn_control_reader,
    spawn_frame_reader, validate_ready,
};

type ControlIncoming = Receiver<Result<WorkerMediaMessage, MediaProtocolError>>;
type FrameIncoming = Receiver<Result<MediaFramePacket, String>>;

pub(crate) struct MediaClientEndpoints {
    pub(crate) control_input: File,
    pub(crate) control_output: File,
    pub(crate) data_output: File,
    pub(crate) frame_input: File,
}

pub(crate) struct RendererMediaDecode {
    pub(crate) report: MediaDecodeReport,
    pub(crate) frame: DecodedMediaFrame,
}

pub(crate) struct MediaClient {
    writer: MediaFrameWriter<File>,
    data_output: File,
    control_incoming: ControlIncoming,
    frame_incoming: FrameIncoming,
    _control_reader: JoinHandle<()>,
    _frame_reader: JoinHandle<()>,
    session: MediaSessionId,
    nonce: Nonce,
    limits: MediaLimits,
    timeout: Duration,
    next_request: u64,
    next_source: u64,
    next_frame: u64,
    active_source: Option<u64>,
}

impl MediaClient {
    pub(crate) fn connect(
        endpoints: MediaClientEndpoints,
        session: MediaSessionId,
        nonce: Nonce,
        startup_timeout: Duration,
        command_timeout: Duration,
    ) -> Result<Self, String> {
        let MediaClientEndpoints {
            control_input,
            control_output,
            data_output,
            frame_input,
        } = endpoints;
        let mut writer = MediaFrameWriter::new(control_output, session);
        let (control_incoming, control_reader) = spawn_control_reader(control_input, session)?;
        let (frame_incoming, frame_reader) = spawn_frame_reader(frame_input, session, nonce)?;
        let limits = MediaLimits::default();
        writer
            .send_browser(&BrowserMediaMessage::Hello { nonce, limits })
            .map_err(|error| format!("send media hello: {error}"))?;
        let ready = receive_from(&control_incoming, "startup", startup_timeout)?;
        let WorkerMediaMessage::Ready {
            nonce: actual,
            containment,
        } = ready
        else {
            return Err("media worker did not send Ready during startup".into());
        };
        validate_ready(nonce, actual, containment)?;
        Ok(Self {
            writer,
            data_output,
            control_incoming,
            frame_incoming,
            _control_reader: control_reader,
            _frame_reader: frame_reader,
            session,
            nonce,
            limits,
            timeout: command_timeout,
            next_request: 1,
            next_source: 1,
            next_frame: 1,
            active_source: None,
        })
    }

    pub(crate) fn decode(
        &mut self,
        bytes: &[u8],
        mut progress: impl FnMut() -> Result<(), String>,
    ) -> Result<RendererMediaDecode, String> {
        if bytes.is_empty() || bytes.len() as u64 > self.limits.max_encoded_queue_bytes {
            return Err("media source exceeds the contained worker queue limit".into());
        }
        let request_id = self.allocate_request()?;
        let source_id = self.next_source;
        self.next_source = checked_next(source_id, "media source identity")?;
        let frame_id = self.allocate_frame()?;
        self.send(BrowserMediaMessage::DecodeSource {
            request_id,
            source_id,
            frame_id,
            encoded_length: bytes.len() as u64,
        })?;
        let source = MediaSourceId::new(source_id).map_err(|error| error.to_string())?;
        let output = self
            .data_output
            .try_clone()
            .map_err(|error| format!("clone media data pipe: {error}"))?;
        let session = self.session;
        let nonce = self.nonce;
        let sent = std::thread::scope(|scope| {
            let sender = scope.spawn(move || {
                MediaDataWriter::new(output, session, nonce).send_source(source, bytes)
            });
            let response = self.receive_with_progress("decode", &mut progress)?;
            let sent = sender
                .join()
                .map_err(|_| "media data writer panicked".to_string())?
                .map_err(|error| format!("deliver encoded media: {error}"));
            Ok::<_, String>((response, sent))
        })?;
        sent.1?;
        let (report, metadata) = match sent.0 {
            WorkerMediaMessage::Decoded {
                request_id: actual,
                report,
                frame,
            } if actual == request_id
                && frame.source_id == source_id
                && frame.frame_id == frame_id =>
            {
                (report, frame)
            }
            WorkerMediaMessage::DecodeFailed {
                request_id: actual,
                error,
            } if actual == request_id => {
                return Err(format!("media worker rejected decode: {error}"));
            }
            _ => return Err("media worker returned the wrong decode response".into()),
        };
        report
            .validate(self.limits)
            .map_err(|error| format!("invalid media decode report: {error}"))?;
        let frame = self.receive_frame(metadata)?;
        self.acknowledge(metadata.source_id, metadata.frame_id)?;
        self.active_source = Some(metadata.source_id);
        Ok(RendererMediaDecode { report, frame })
    }

    pub(crate) fn decode_tracks(
        &mut self,
        video_bytes: &[u8],
        audio_bytes: &[u8],
        mut progress: impl FnMut() -> Result<(), String>,
    ) -> Result<RendererMediaDecode, String> {
        let encoded_length = video_bytes
            .len()
            .checked_add(audio_bytes.len())
            .ok_or_else(|| "adaptive media source length overflowed".to_string())?;
        if video_bytes.is_empty()
            || audio_bytes.is_empty()
            || encoded_length as u64 > self.limits.max_encoded_queue_bytes
        {
            return Err("adaptive media source exceeds the contained worker queue limit".into());
        }
        let request_id = self.allocate_request()?;
        let video_source_id = self.next_source;
        let audio_source_id = checked_next(video_source_id, "media source identity")?;
        self.next_source = checked_next(audio_source_id, "media source identity")?;
        let frame_id = self.allocate_frame()?;
        self.send(BrowserMediaMessage::DecodeTracks {
            request_id,
            video_source_id,
            audio_source_id,
            frame_id,
            video_length: video_bytes.len() as u64,
            audio_length: audio_bytes.len() as u64,
        })?;
        let video_source =
            MediaSourceId::new(video_source_id).map_err(|error| error.to_string())?;
        let audio_source =
            MediaSourceId::new(audio_source_id).map_err(|error| error.to_string())?;
        let output = self
            .data_output
            .try_clone()
            .map_err(|error| format!("clone media data pipe: {error}"))?;
        let session = self.session;
        let nonce = self.nonce;
        let sent = std::thread::scope(|scope| {
            let sender = scope.spawn(move || {
                let mut writer = MediaDataWriter::new(output, session, nonce);
                writer.send_source(video_source, video_bytes)?;
                writer.send_source(audio_source, audio_bytes)
            });
            let response = self.receive_with_progress("decode adaptive tracks", &mut progress)?;
            let sent = sender
                .join()
                .map_err(|_| "media data writer panicked".to_string())?
                .map_err(|error| format!("deliver adaptive media tracks: {error}"));
            Ok::<_, String>((response, sent))
        })?;
        sent.1?;
        let (report, metadata) = match sent.0 {
            WorkerMediaMessage::Decoded {
                request_id: actual,
                report,
                frame,
            } if actual == request_id
                && frame.source_id == video_source_id
                && frame.frame_id == frame_id =>
            {
                (report, frame)
            }
            WorkerMediaMessage::DecodeFailed {
                request_id: actual,
                error,
            } if actual == request_id => {
                return Err(format!("media worker rejected adaptive decode: {error}"));
            }
            _ => return Err("media worker returned the wrong adaptive decode response".into()),
        };
        report
            .validate(self.limits)
            .map_err(|error| format!("invalid adaptive media decode report: {error}"))?;
        let frame = self.receive_frame(metadata)?;
        self.acknowledge(metadata.source_id, metadata.frame_id)?;
        self.active_source = Some(metadata.source_id);
        Ok(RendererMediaDecode { report, frame })
    }

    pub(crate) fn next_frame(
        &mut self,
        source_id: u64,
    ) -> Result<Option<DecodedMediaFrame>, String> {
        let frame_id = self.allocate_frame()?;
        self.send(BrowserMediaMessage::RequestFrame {
            source_id,
            frame_id,
        })?;
        let metadata = match self.receive("request frame")? {
            WorkerMediaMessage::FrameReady { frame }
                if frame.source_id == source_id && frame.frame_id == frame_id =>
            {
                frame
            }
            WorkerMediaMessage::EndOfStream { source_id: actual } if actual == source_id => {
                return Ok(None);
            }
            WorkerMediaMessage::DecodeFailed { request_id, error } if request_id == frame_id => {
                return Err(format!("media worker failed to decode frame: {error}"));
            }
            _ => return Err("media worker returned the wrong frame response".into()),
        };
        let frame = self.receive_frame(metadata)?;
        self.acknowledge(source_id, frame_id)?;
        Ok(Some(frame))
    }

    pub(crate) fn set_playback(
        &mut self,
        source_id: u64,
        playing: bool,
        volume_millis: u16,
    ) -> Result<MediaPlaybackState, String> {
        self.send(BrowserMediaMessage::SetPlayback {
            source_id,
            playing,
            volume_millis,
        })?;
        self.receive_playback_state(source_id, "set playback")
    }

    pub(crate) fn playback_state(&mut self, source_id: u64) -> Result<MediaPlaybackState, String> {
        self.send(BrowserMediaMessage::PlaybackState { source_id })?;
        self.receive_playback_state(source_id, "query playback")
    }

    pub(crate) fn seek_playback(
        &mut self,
        source_id: u64,
        position_100ns: u64,
    ) -> Result<MediaPlaybackState, String> {
        self.send(BrowserMediaMessage::SeekPlayback {
            source_id,
            position_100ns,
        })?;
        self.receive_playback_state(source_id, "seek playback")
    }

    fn acknowledge(&mut self, source_id: u64, frame_id: u64) -> Result<(), String> {
        self.send(BrowserMediaMessage::AcknowledgeFrame {
            source_id,
            frame_id,
        })?;
        match self.receive("acknowledge frame")? {
            WorkerMediaMessage::FrameAcknowledged {
                source_id: actual_source,
                frame_id: actual_frame,
            } if actual_source == source_id && actual_frame == frame_id => Ok(()),
            _ => Err("media worker returned a stale frame acknowledgement".into()),
        }
    }

    fn receive_playback_state(
        &self,
        source_id: u64,
        operation: &str,
    ) -> Result<MediaPlaybackState, String> {
        match self.receive(operation)? {
            WorkerMediaMessage::PlaybackState(state) if state.source_id == source_id => {
                state
                    .validate()
                    .map_err(|error| format!("invalid media playback state: {error}"))?;
                Ok(state)
            }
            _ => Err("media worker returned stale playback state".into()),
        }
    }

    fn receive_frame(
        &mut self,
        metadata: crate::media_protocol::MediaVideoFrameMetadata,
    ) -> Result<DecodedMediaFrame, String> {
        let packet = self
            .frame_incoming
            .recv_timeout(self.timeout)
            .map_err(|error| format!("decoded media frame timed out or disconnected: {error}"))??;
        if packet.metadata != metadata {
            return Err("media frame metadata disagreed with control response".into());
        }
        let bgra = nv12_to_bgra(metadata, &packet.nv12)
            .map_err(|error| format!("convert decoded NV12 frame: {error}"))?
            .bgra;
        Ok(DecodedMediaFrame {
            metadata,
            nv12: packet.nv12,
            bgra,
        })
    }

    fn send(&mut self, message: BrowserMediaMessage) -> Result<(), String> {
        self.writer
            .send_browser(&message)
            .map_err(|error| format!("send media command: {error}"))
    }

    fn receive(&self, operation: &str) -> Result<WorkerMediaMessage, String> {
        receive_from(&self.control_incoming, operation, self.timeout)
    }

    fn receive_with_progress(
        &self,
        operation: &str,
        progress: &mut impl FnMut() -> Result<(), String>,
    ) -> Result<WorkerMediaMessage, String> {
        receive_from_with_progress(&self.control_incoming, operation, self.timeout, progress)
    }

    fn allocate_request(&mut self) -> Result<u64, String> {
        let current = self.next_request;
        self.next_request = checked_next(current, "media request identity")?;
        Ok(current)
    }

    fn allocate_frame(&mut self) -> Result<u64, String> {
        let current = self.next_frame;
        self.next_frame = checked_next(current, "media frame identity")?;
        Ok(current)
    }
}
