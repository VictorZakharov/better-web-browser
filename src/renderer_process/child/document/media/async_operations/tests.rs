use super::*;
use crate::media_frame_protocol::{MediaPixelFormat, MediaVideoFrameMetadata};
use crate::media_process::DecodedMediaFrame;
use crate::media_protocol::{MediaBufferedExtent, MediaCodecFamily, MediaDecodeReport};

#[test]
fn discarded_successful_decode_forgets_the_replaced_source_without_playback_commands() {
    let mut playback = Some(previous_playback());
    let completion = MediaOperationCompletion::Decoded(Ok(decoded_replacement()));
    assert!(discard_replaced_media(&mut playback, &completion));
    assert!(playback.is_none());
    // The same policy covers a completion that outlives the entire document, when no
    // previous playback remains but the connection's video registration must be cleared.
    assert!(discard_replaced_media(&mut playback, &completion));
}

#[test]
fn discarded_failed_decode_or_append_keeps_the_still_installed_source() {
    let completions = [
        MediaOperationCompletion::Decoded(Err("owned decode rejection".into())),
        MediaOperationCompletion::Appended(Ok((10, 333_333, buffered()))),
        MediaOperationCompletion::Appended(Err("owned append rejection".into())),
    ];
    for completion in completions {
        let mut playback = Some(previous_playback());
        assert!(!discard_replaced_media(&mut playback, &completion));
        let installed = playback.unwrap();
        assert_eq!(installed.source_id, 1);
        assert!(installed.playing);
        assert_eq!(installed.clock_100ns, 100);
    }
}

fn previous_playback() -> MediaPlayback {
    let dom = crate::engine::dom::parse("<video></video>");
    let node = dom.elements_named("video").next().unwrap().id();
    MediaPlayback {
        node,
        source_id: 1,
        clock_100ns: 100,
        frame_end_100ns: 333_333,
        duration_100ns: 333_333,
        buffered: buffered(),
        playing: true,
        ended: false,
        video_ended: false,
        width: 2,
        height: 2,
        mime_type: "video/mp4".into(),
        encoded_bytes: 2,
        frames_submitted: 1,
        dropped_frames: 0,
    }
}

fn buffered() -> MediaBufferedExtent {
    MediaBufferedExtent {
        video_start_100ns: 0,
        video_end_100ns: 333_333,
        audio_start_100ns: 0,
        audio_end_100ns: 333_333,
    }
}

fn decoded_replacement() -> RendererMediaDecode {
    RendererMediaDecode {
        report: MediaDecodeReport {
            buffered: buffered(),
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
        },
        frame: DecodedMediaFrame {
            metadata: MediaVideoFrameMetadata {
                source_id: 2,
                frame_id: 2,
                timestamp_100ns: 0,
                duration_100ns: 333_333,
                width: 2,
                height: 2,
                stride: 2,
                format: MediaPixelFormat::Nv12,
                data_length: 6,
            },
            nv12: vec![16, 16, 16, 16, 128, 128],
            bgra: vec![0; 16].into(),
        },
    }
}
