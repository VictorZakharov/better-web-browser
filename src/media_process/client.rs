//! Renderer-side client for a browser-launched contained media worker.

use super::broker::DecodedMediaFrame;
use crate::media_data_protocol::{MediaDataWriter, MediaSourceId};
use crate::media_frame_protocol::{
    MediaFramePacket, MediaFrameReader as DecodedFrameReader, nv12_to_bgra,
};
use crate::media_protocol::{
    BrowserMediaMessage, ContainmentReport, MediaDecodeReport, MediaFrameReader, MediaFrameWriter,
    MediaLimits, MediaPlaybackState, MediaProtocolError, MediaSessionId, Nonce, WorkerMediaMessage,
};
use std::fs::File;
use std::sync::mpsc::{self, Receiver};
use std::thread::JoinHandle;
use std::time::Duration;

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
        })
    }

    pub(crate) fn decode(&mut self, bytes: &[u8]) -> Result<RendererMediaDecode, String> {
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
            let response = self.receive("decode")?;
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
            } if actual == request_id => (report, frame),
            _ => return Err("media worker returned the wrong decode response".into()),
        };
        report
            .validate(self.limits)
            .map_err(|error| format!("invalid media decode report: {error}"))?;
        let frame = self.receive_frame(metadata)?;
        self.acknowledge(metadata.source_id, metadata.frame_id)?;
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

fn checked_next(value: u64, label: &str) -> Result<u64, String> {
    value
        .checked_add(1)
        .ok_or_else(|| format!("{label} exhausted"))
}

fn receive_from(
    incoming: &ControlIncoming,
    operation: &str,
    timeout: Duration,
) -> Result<WorkerMediaMessage, String> {
    incoming
        .recv_timeout(timeout)
        .map_err(|error| format!("media {operation} timed out or disconnected: {error}"))?
        .map_err(|error| format!("media {operation} protocol failed: {error}"))
}

fn spawn_control_reader(
    input: File,
    session: MediaSessionId,
) -> Result<(ControlIncoming, JoinHandle<()>), String> {
    let (sender, receiver) = mpsc::sync_channel(1);
    let thread = std::thread::Builder::new()
        .name("breeze-renderer-media-control".into())
        .spawn(move || {
            let mut reader = MediaFrameReader::new(input, session);
            loop {
                let message = reader.read_worker();
                let failed = message.is_err();
                if sender.send(message).is_err() || failed {
                    break;
                }
            }
        })
        .map_err(|error| format!("start renderer media control reader: {error}"))?;
    Ok((receiver, thread))
}

fn spawn_frame_reader(
    input: File,
    session: MediaSessionId,
    nonce: Nonce,
) -> Result<(FrameIncoming, JoinHandle<()>), String> {
    let (sender, receiver) = mpsc::sync_channel(1);
    let thread = std::thread::Builder::new()
        .name("breeze-renderer-media-frames".into())
        .spawn(move || {
            let mut reader = DecodedFrameReader::new(input, session, nonce);
            loop {
                let frame = reader.read_next_frame().map_err(|error| error.to_string());
                let failed = frame.is_err();
                if sender.send(frame).is_err() || failed {
                    break;
                }
            }
        })
        .map_err(|error| format!("start renderer media frame reader: {error}"))?;
    Ok((receiver, thread))
}

fn validate_ready(
    expected: Nonce,
    actual: Nonce,
    containment: ContainmentReport,
) -> Result<(), String> {
    if expected != actual {
        return Err("media worker returned a stale bootstrap nonce".into());
    }
    if !containment.app_container
        || !containment.no_console_window
        || !containment.minimal_environment
    {
        return Err("media worker did not satisfy its containment contract".into());
    }
    Ok(())
}
