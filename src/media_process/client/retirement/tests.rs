use super::*;
use crate::media_frame_protocol::{MediaPixelFormat, MediaVideoFrameMetadata};
use crate::media_protocol::{MediaBufferedExtent, MediaCodecFamily};
use std::sync::mpsc;

// Exercise the production client with authored protocol responses, without spawning a browser
// or media worker. NUL discards outbound bytes; response identity checks still enforce ordering.
struct Fixture {
    client: MediaClient,
    controls: mpsc::Sender<Result<WorkerMediaMessage, MediaProtocolError>>,
    frames: mpsc::Sender<Result<MediaFramePacket, String>>,
}

impl Fixture {
    fn new() -> Self {
        let output = || File::options().read(true).write(true).open("NUL").unwrap();
        let (controls, control_incoming) = mpsc::channel();
        let (frames, frame_incoming) = mpsc::channel();
        let session = MediaSessionId::new(1).unwrap();
        Self {
            client: MediaClient {
                writer: MediaFrameWriter::new(output(), session),
                data_output: output(),
                control_incoming,
                frame_incoming,
                _control_reader: std::thread::spawn(|| {}),
                _frame_reader: std::thread::spawn(|| {}),
                session,
                nonce: Nonce::new([7; 32]),
                limits: MediaLimits::default(),
                timeout: Duration::from_millis(100),
                next_request: 1,
                next_source: 1,
                next_frame: 1,
                active_source: None,
            },
            controls,
            frames,
        }
    }

    fn decode(&mut self, adaptive: bool) -> u64 {
        let source_id = self.client.next_source;
        let frame_id = self.client.next_frame;
        let metadata = MediaVideoFrameMetadata {
            source_id,
            frame_id,
            timestamp_100ns: 0,
            duration_100ns: 333_333,
            width: 2,
            height: 2,
            stride: 2,
            format: MediaPixelFormat::Nv12,
            data_length: 6,
        };
        self.controls
            .send(Ok(WorkerMediaMessage::Decoded {
                request_id: self.client.next_request,
                report: report(),
                frame: metadata,
            }))
            .unwrap();
        self.controls
            .send(Ok(WorkerMediaMessage::FrameAcknowledged {
                source_id,
                frame_id,
            }))
            .unwrap();
        self.frames
            .send(Ok(MediaFramePacket {
                metadata,
                nv12: vec![16, 16, 16, 16, 128, 128],
            }))
            .unwrap();
        let decoded = if adaptive {
            self.client.decode_tracks(&[1], &[2], || Ok(()))
        } else {
            self.client.decode(&[1, 2], || Ok(()))
        }
        .unwrap();
        assert_eq!(decoded.frame.metadata.source_id, source_id);
        assert_eq!(self.client.active_source, Some(source_id));
        source_id
    }

    fn playback_response(&self, source_id: u64, playing: bool) {
        self.controls
            .send(Ok(WorkerMediaMessage::PlaybackState(MediaPlaybackState {
                source_id,
                position_100ns: 0,
                duration_100ns: 333_333,
                playing,
                ended: false,
            })))
            .unwrap();
    }
}

#[test]
fn retirement_after_replacement_does_not_send_a_stale_playback_command() {
    let mut fixture = Fixture::new();
    let retired = fixture.decode(false);
    let replacement = fixture.decode(true);
    assert_ne!(retired, replacement);
    fixture.playback_response(replacement, false);

    assert!(!fixture.client.pause_retired_source(retired).unwrap());
    // A stale command would consume this response and fail its source identity check. It must
    // remain available for the legitimate pause of the newly installed source.
    assert!(fixture.client.pause_retired_source(replacement).unwrap());
}

#[test]
fn retirement_before_replacement_pauses_old_source_without_poisoning_new_source() {
    let mut fixture = Fixture::new();
    let retired = fixture.decode(true);
    fixture.playback_response(retired, false);
    assert!(fixture.client.pause_retired_source(retired).unwrap());

    let replacement = fixture.decode(false);
    fixture.playback_response(replacement, true);
    assert!(!fixture.client.pause_retired_source(retired).unwrap());
    let state = fixture.client.set_playback(replacement, true, 0).unwrap();
    assert!(state.playing);
}

#[test]
fn rejected_replacement_does_not_retire_the_still_installed_source() {
    let mut fixture = Fixture::new();
    assert!(!fixture.client.pause_retired_source(1).unwrap());
    let installed = fixture.decode(false);
    let rejected = fixture.client.next_source;
    fixture
        .controls
        .send(Ok(WorkerMediaMessage::DecodeFailed {
            request_id: fixture.client.next_request,
            error: "owned fixture rejection".into(),
        }))
        .unwrap();
    assert!(fixture.client.decode_tracks(&[1], &[2], || Ok(())).is_err());
    assert_eq!(fixture.client.active_source, Some(installed));
    fixture.playback_response(installed, false);
    assert!(!fixture.client.pause_retired_source(rejected).unwrap());
    assert!(fixture.client.pause_retired_source(installed).unwrap());
}

fn report() -> MediaDecodeReport {
    MediaDecodeReport {
        buffered: MediaBufferedExtent {
            video_start_100ns: 0,
            video_end_100ns: 333_333,
            audio_start_100ns: 0,
            audio_end_100ns: 333_333,
        },
        encoded_bytes: 2,
        video_codec: MediaCodecFamily::H264,
        audio_codec: MediaCodecFamily::AacLc,
        source_reader_hresult: 0,
        video_decode_hresult: 0,
        audio_decode_hresult: 0,
        video_width: 2,
        video_height: 2,
        audio_sample_rate: 48_000,
        audio_channels: 1,
        video_samples: 1,
        audio_samples: 1,
        video_decoded_bytes: 6,
        audio_decoded_bytes: 2,
        video_first_timestamp_100ns: 0,
        video_last_timestamp_100ns: 0,
        audio_first_timestamp_100ns: 0,
        audio_last_timestamp_100ns: 0,
        duration_100ns: 333_333,
        decode_micros: 1,
    }
}
