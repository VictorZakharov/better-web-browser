use super::wire::{
    boolean, encode_frame_metadata, encode_limits, encode_test, require_nonzero, vec_i32, vec_i64,
    vec_u16, vec_u64,
};
use super::{
    BROWSER_ACKNOWLEDGE_FRAME, BROWSER_DECODE_SOURCE, BROWSER_HELLO, BROWSER_PING,
    BROWSER_PLAYBACK_STATE, BROWSER_PROBE, BROWSER_REQUEST_FRAME, BROWSER_SET_PLAYBACK,
    BROWSER_SHUTDOWN, BROWSER_TEST, MediaProtocolError, WORKER_CAPABILITY, WORKER_DECODED,
    WORKER_END_OF_STREAM, WORKER_FRAME_ACKNOWLEDGED, WORKER_FRAME_READY, WORKER_PLAYBACK_STATE,
    WORKER_PONG, WORKER_READY, WORKER_RESTRICTIONS, WORKER_SHUTDOWN_COMPLETE,
};
use crate::media_protocol::{BrowserMediaMessage, MediaLimits, WorkerMediaMessage};

pub(super) fn browser(message: BrowserMediaMessage) -> Result<(u16, Vec<u8>), MediaProtocolError> {
    let mut payload = Vec::new();
    let kind = match message {
        BrowserMediaMessage::Hello { nonce, limits } => {
            limits.validate()?;
            payload.extend_from_slice(nonce.as_bytes());
            encode_limits(&mut payload, limits);
            BROWSER_HELLO
        }
        BrowserMediaMessage::Ping(token) => {
            vec_u64(&mut payload, token);
            BROWSER_PING
        }
        BrowserMediaMessage::Shutdown => BROWSER_SHUTDOWN,
        BrowserMediaMessage::Probe { request_id } => {
            require_nonzero(request_id, "probe request")?;
            vec_u64(&mut payload, request_id);
            BROWSER_PROBE
        }
        BrowserMediaMessage::DecodeSource {
            request_id,
            source_id,
            frame_id,
            encoded_length,
        } => {
            require_nonzero(request_id, "decode request")?;
            require_nonzero(source_id, "media source")?;
            require_nonzero(frame_id, "frame generation")?;
            require_nonzero(encoded_length, "encoded length")?;
            if encoded_length > MediaLimits::default().max_encoded_queue_bytes {
                return Err(MediaProtocolError::InvalidPayload("encoded length"));
            }
            vec_u64(&mut payload, request_id);
            vec_u64(&mut payload, source_id);
            vec_u64(&mut payload, frame_id);
            vec_u64(&mut payload, encoded_length);
            BROWSER_DECODE_SOURCE
        }
        BrowserMediaMessage::AcknowledgeFrame {
            source_id,
            frame_id,
        } => {
            require_nonzero(source_id, "frame source")?;
            require_nonzero(frame_id, "frame generation")?;
            vec_u64(&mut payload, source_id);
            vec_u64(&mut payload, frame_id);
            BROWSER_ACKNOWLEDGE_FRAME
        }
        BrowserMediaMessage::RequestFrame {
            source_id,
            frame_id,
        } => {
            require_nonzero(source_id, "frame source")?;
            require_nonzero(frame_id, "frame generation")?;
            vec_u64(&mut payload, source_id);
            vec_u64(&mut payload, frame_id);
            BROWSER_REQUEST_FRAME
        }
        BrowserMediaMessage::SetPlayback {
            source_id,
            playing,
            volume_millis,
        } => {
            require_nonzero(source_id, "playback source")?;
            if volume_millis > 1_000 {
                return Err(MediaProtocolError::InvalidPayload("playback volume"));
            }
            vec_u64(&mut payload, source_id);
            boolean(&mut payload, playing);
            vec_u16(&mut payload, volume_millis);
            BROWSER_SET_PLAYBACK
        }
        BrowserMediaMessage::PlaybackState { source_id } => {
            require_nonzero(source_id, "playback source")?;
            vec_u64(&mut payload, source_id);
            BROWSER_PLAYBACK_STATE
        }
        BrowserMediaMessage::Test(command) => {
            encode_test(&mut payload, command);
            BROWSER_TEST
        }
    };
    Ok((kind, payload))
}

pub(super) fn worker(message: WorkerMediaMessage) -> Result<(u16, Vec<u8>), MediaProtocolError> {
    let mut payload = Vec::new();
    let kind = match message {
        WorkerMediaMessage::Ready { nonce, containment } => {
            payload.extend_from_slice(nonce.as_bytes());
            boolean(&mut payload, containment.app_container);
            boolean(&mut payload, containment.no_console_window);
            boolean(&mut payload, containment.minimal_environment);
            WORKER_READY
        }
        WorkerMediaMessage::Pong(token) => {
            vec_u64(&mut payload, token);
            WORKER_PONG
        }
        WorkerMediaMessage::ShutdownComplete => WORKER_SHUTDOWN_COMPLETE,
        WorkerMediaMessage::Capability { request_id, report } => {
            require_nonzero(request_id, "capability request")?;
            report.validate(MediaLimits::default())?;
            vec_u64(&mut payload, request_id);
            vec_i32(&mut payload, report.startup_hresult);
            vec_i32(&mut payload, report.h264_hresult);
            vec_i32(&mut payload, report.aac_hresult);
            vec_u16(&mut payload, report.h264_decoders);
            vec_u16(&mut payload, report.aac_decoders);
            vec_u64(&mut payload, report.probe_micros);
            WORKER_CAPABILITY
        }
        WorkerMediaMessage::Decoded {
            request_id,
            report,
            frame,
        } => {
            require_nonzero(request_id, "decode request")?;
            report.validate(MediaLimits::default())?;
            frame
                .validate()
                .map_err(|_| MediaProtocolError::InvalidPayload("video frame metadata"))?;
            vec_u64(&mut payload, request_id);
            vec_u64(&mut payload, report.encoded_bytes);
            vec_u16(&mut payload, report.video_codec.wire_code());
            vec_u16(&mut payload, report.audio_codec.wire_code());
            vec_i32(&mut payload, report.source_reader_hresult);
            vec_i32(&mut payload, report.video_decode_hresult);
            vec_i32(&mut payload, report.audio_decode_hresult);
            super::wire::vec_u32(&mut payload, report.video_width);
            super::wire::vec_u32(&mut payload, report.video_height);
            super::wire::vec_u32(&mut payload, report.audio_sample_rate);
            vec_u16(&mut payload, report.audio_channels);
            super::wire::vec_u32(&mut payload, report.video_samples);
            super::wire::vec_u32(&mut payload, report.audio_samples);
            vec_u64(&mut payload, report.video_decoded_bytes);
            vec_u64(&mut payload, report.audio_decoded_bytes);
            vec_i64(&mut payload, report.video_first_timestamp_100ns);
            vec_i64(&mut payload, report.video_last_timestamp_100ns);
            vec_i64(&mut payload, report.audio_first_timestamp_100ns);
            vec_i64(&mut payload, report.audio_last_timestamp_100ns);
            vec_u64(&mut payload, report.duration_100ns);
            vec_u64(&mut payload, report.decode_micros);
            encode_frame_metadata(&mut payload, frame);
            WORKER_DECODED
        }
        WorkerMediaMessage::FrameAcknowledged {
            source_id,
            frame_id,
        } => {
            require_nonzero(source_id, "frame source")?;
            require_nonzero(frame_id, "frame generation")?;
            vec_u64(&mut payload, source_id);
            vec_u64(&mut payload, frame_id);
            WORKER_FRAME_ACKNOWLEDGED
        }
        WorkerMediaMessage::FrameReady { frame } => {
            frame
                .validate()
                .map_err(|_| MediaProtocolError::InvalidPayload("video frame metadata"))?;
            encode_frame_metadata(&mut payload, frame);
            WORKER_FRAME_READY
        }
        WorkerMediaMessage::EndOfStream { source_id } => {
            require_nonzero(source_id, "frame source")?;
            vec_u64(&mut payload, source_id);
            WORKER_END_OF_STREAM
        }
        WorkerMediaMessage::PlaybackState(state) => {
            state.validate()?;
            vec_u64(&mut payload, state.source_id);
            vec_u64(&mut payload, state.position_100ns);
            vec_u64(&mut payload, state.duration_100ns);
            boolean(&mut payload, state.playing);
            boolean(&mut payload, state.ended);
            WORKER_PLAYBACK_STATE
        }
        WorkerMediaMessage::Restrictions(report) => {
            boolean(&mut payload, report.child_launch_denied);
            boolean(&mut payload, report.loopback_denied);
            boolean(&mut payload, report.internet_denied);
            vec_i32(&mut payload, report.child_error);
            vec_i32(&mut payload, report.loopback_error);
            vec_i32(&mut payload, report.internet_error);
            WORKER_RESTRICTIONS
        }
    };
    Ok((kind, payload))
}
