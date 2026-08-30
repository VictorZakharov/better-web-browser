use super::wire::{Cursor, decode_frame_metadata, decode_limits, decode_test};
use super::{
    BROWSER_ACKNOWLEDGE_FRAME, BROWSER_DECODE_SOURCE, BROWSER_HELLO, BROWSER_PING,
    BROWSER_PLAYBACK_STATE, BROWSER_PROBE, BROWSER_REQUEST_FRAME, BROWSER_SEEK_PLAYBACK,
    BROWSER_SET_PLAYBACK, BROWSER_SHUTDOWN, BROWSER_TEST, MediaProtocolError, WORKER_CAPABILITY,
    WORKER_DECODED, WORKER_END_OF_STREAM, WORKER_FRAME_ACKNOWLEDGED, WORKER_FRAME_READY,
    WORKER_PLAYBACK_STATE, WORKER_PONG, WORKER_READY, WORKER_RESTRICTIONS,
    WORKER_SHUTDOWN_COMPLETE,
};
use crate::media_protocol::{
    BrowserMediaMessage, ContainmentReport, MediaCapabilityReport, MediaCodecFamily,
    MediaDecodeReport, MediaLimits, MediaPlaybackState, MediaRestrictionReport, Nonce,
    WorkerMediaMessage,
};

pub(super) fn browser(
    kind: u16,
    payload: &[u8],
) -> Result<BrowserMediaMessage, MediaProtocolError> {
    let mut cursor = Cursor::new(payload);
    let message = match kind {
        BROWSER_HELLO => BrowserMediaMessage::Hello {
            nonce: Nonce::new(cursor.array()?),
            limits: decode_limits(&mut cursor)?,
        },
        BROWSER_PING => BrowserMediaMessage::Ping(cursor.u64()?),
        BROWSER_SHUTDOWN => BrowserMediaMessage::Shutdown,
        BROWSER_PROBE => BrowserMediaMessage::Probe {
            request_id: cursor.nonzero_u64("probe request")?,
        },
        BROWSER_DECODE_SOURCE => {
            let request_id = cursor.nonzero_u64("decode request")?;
            let source_id = cursor.nonzero_u64("media source")?;
            let frame_id = cursor.nonzero_u64("frame generation")?;
            let encoded_length = cursor.nonzero_u64("encoded length")?;
            if encoded_length > MediaLimits::default().max_encoded_queue_bytes {
                return Err(MediaProtocolError::InvalidPayload("encoded length"));
            }
            BrowserMediaMessage::DecodeSource {
                request_id,
                source_id,
                frame_id,
                encoded_length,
            }
        }
        BROWSER_ACKNOWLEDGE_FRAME => BrowserMediaMessage::AcknowledgeFrame {
            source_id: cursor.nonzero_u64("frame source")?,
            frame_id: cursor.nonzero_u64("frame generation")?,
        },
        BROWSER_REQUEST_FRAME => BrowserMediaMessage::RequestFrame {
            source_id: cursor.nonzero_u64("frame source")?,
            frame_id: cursor.nonzero_u64("frame generation")?,
        },
        BROWSER_SET_PLAYBACK => {
            let source_id = cursor.nonzero_u64("playback source")?;
            let playing = cursor.boolean()?;
            let volume_millis = cursor.u16()?;
            if volume_millis > 1_000 {
                return Err(MediaProtocolError::InvalidPayload("playback volume"));
            }
            BrowserMediaMessage::SetPlayback {
                source_id,
                playing,
                volume_millis,
            }
        }
        BROWSER_PLAYBACK_STATE => BrowserMediaMessage::PlaybackState {
            source_id: cursor.nonzero_u64("playback source")?,
        },
        BROWSER_SEEK_PLAYBACK => {
            let source_id = cursor.nonzero_u64("playback source")?;
            let position_100ns = cursor.u64()?;
            if position_100ns > crate::limits::MAX_MEDIA_DURATION_100NS {
                return Err(MediaProtocolError::InvalidPayload("playback seek position"));
            }
            BrowserMediaMessage::SeekPlayback {
                source_id,
                position_100ns,
            }
        }
        BROWSER_TEST => BrowserMediaMessage::Test(decode_test(&mut cursor)?),
        _ => return Err(MediaProtocolError::UnexpectedMessage(kind)),
    };
    cursor.finish()?;
    if let BrowserMediaMessage::Hello { limits, .. } = message {
        limits.validate()?;
    }
    Ok(message)
}

pub(super) fn worker(kind: u16, payload: &[u8]) -> Result<WorkerMediaMessage, MediaProtocolError> {
    let mut cursor = Cursor::new(payload);
    let message = match kind {
        WORKER_READY => WorkerMediaMessage::Ready {
            nonce: Nonce::new(cursor.array()?),
            containment: ContainmentReport {
                app_container: cursor.boolean()?,
                no_console_window: cursor.boolean()?,
                minimal_environment: cursor.boolean()?,
            },
        },
        WORKER_PONG => WorkerMediaMessage::Pong(cursor.u64()?),
        WORKER_SHUTDOWN_COMPLETE => WorkerMediaMessage::ShutdownComplete,
        WORKER_CAPABILITY => WorkerMediaMessage::Capability {
            request_id: cursor.nonzero_u64("capability request")?,
            report: MediaCapabilityReport {
                startup_hresult: cursor.i32()?,
                h264_hresult: cursor.i32()?,
                aac_hresult: cursor.i32()?,
                h264_decoders: cursor.u16()?,
                aac_decoders: cursor.u16()?,
                probe_micros: cursor.u64()?,
            },
        },
        WORKER_DECODED => {
            let request_id = cursor.nonzero_u64("decode request")?;
            let report = MediaDecodeReport {
                encoded_bytes: cursor.u64()?,
                video_codec: MediaCodecFamily::from_wire(cursor.u16()?)?,
                audio_codec: MediaCodecFamily::from_wire(cursor.u16()?)?,
                source_reader_hresult: cursor.i32()?,
                video_decode_hresult: cursor.i32()?,
                audio_decode_hresult: cursor.i32()?,
                video_width: cursor.u32()?,
                video_height: cursor.u32()?,
                audio_sample_rate: cursor.u32()?,
                audio_channels: cursor.u16()?,
                video_samples: cursor.u32()?,
                audio_samples: cursor.u32()?,
                video_decoded_bytes: cursor.u64()?,
                audio_decoded_bytes: cursor.u64()?,
                video_first_timestamp_100ns: cursor.i64()?,
                video_last_timestamp_100ns: cursor.i64()?,
                audio_first_timestamp_100ns: cursor.i64()?,
                audio_last_timestamp_100ns: cursor.i64()?,
                duration_100ns: cursor.u64()?,
                decode_micros: cursor.u64()?,
            };
            let frame = decode_frame_metadata(&mut cursor)?;
            WorkerMediaMessage::Decoded {
                request_id,
                report,
                frame,
            }
        }
        WORKER_FRAME_ACKNOWLEDGED => WorkerMediaMessage::FrameAcknowledged {
            source_id: cursor.nonzero_u64("frame source")?,
            frame_id: cursor.nonzero_u64("frame generation")?,
        },
        WORKER_FRAME_READY => WorkerMediaMessage::FrameReady {
            frame: decode_frame_metadata(&mut cursor)?,
        },
        WORKER_END_OF_STREAM => WorkerMediaMessage::EndOfStream {
            source_id: cursor.nonzero_u64("frame source")?,
        },
        WORKER_PLAYBACK_STATE => WorkerMediaMessage::PlaybackState(MediaPlaybackState {
            source_id: cursor.nonzero_u64("playback source")?,
            position_100ns: cursor.u64()?,
            duration_100ns: cursor.u64()?,
            playing: cursor.boolean()?,
            ended: cursor.boolean()?,
        }),
        WORKER_RESTRICTIONS => WorkerMediaMessage::Restrictions(MediaRestrictionReport {
            child_launch_denied: cursor.boolean()?,
            loopback_denied: cursor.boolean()?,
            internet_denied: cursor.boolean()?,
            child_error: cursor.i32()?,
            loopback_error: cursor.i32()?,
            internet_error: cursor.i32()?,
        }),
        _ => return Err(MediaProtocolError::UnexpectedMessage(kind)),
    };
    cursor.finish()?;
    if let WorkerMediaMessage::Capability { report, .. } = message {
        report.validate(MediaLimits::default())?;
    }
    if let WorkerMediaMessage::Decoded { report, frame, .. } = message {
        report.validate(MediaLimits::default())?;
        frame
            .validate()
            .map_err(|_| MediaProtocolError::InvalidPayload("video frame metadata"))?;
    }
    if let WorkerMediaMessage::FrameReady { frame } = message {
        frame
            .validate()
            .map_err(|_| MediaProtocolError::InvalidPayload("video frame metadata"))?;
    }
    if let WorkerMediaMessage::PlaybackState(state) = message {
        state.validate()?;
    }
    Ok(message)
}
