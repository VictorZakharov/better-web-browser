use super::*;
use std::time::Instant;

#[test]
fn video_starvation_holds_the_audio_clock_until_video_data_arrives() {
    let _serial = SERIAL.lock().unwrap_or_else(|error| error.into_inner());
    let mut launch = options();
    launch.enable_media = true;
    let mut session = RendererSession::launch(launch).unwrap();
    let document = better_web_browser::renderer_protocol::DocumentId::new(198).unwrap();
    let video = include_str!("../../fixtures/media/test-1s-video-fragmented.mp4.base64")
        .split_whitespace()
        .collect::<String>();
    let audio = include_str!("../../fixtures/media/test-1s-audio-fragmented.mp4.base64")
        .split_whitespace()
        .collect::<String>();
    let html = format!(
        r#"<!doctype html><video id="movie" muted width="320" height="240"></video><script>
        function shifted(text, seconds) {{
            const data = Uint8Array.from(atob(text), c => c.charCodeAt(0));
            const view = new DataView(data.buffer);
            const tag = name => {{
                for (let i = 0; i < data.length - 4; i++)
                    if ([...name].every((c,n) => data[i+n] === c.charCodeAt(0))) return i;
                throw new Error('fixture box missing');
            }};
            const scale = view.getUint32(tag('mdhd') + 16);
            view.setBigUint64(tag('tfdt') + 8, BigInt(scale) * BigInt(seconds));
            return data;
        }}
        const source = new MediaSource();
        let video, audio, audioChunks = 0, resumed = false;
        movie.addEventListener('error', () => console.log('unexpected media error'));
        movie.addEventListener('seeking', () => console.log('unexpected author seek'));
        movie.addEventListener('playing', () => fetch('/playing-ack').then(() =>
            console.log('acknowledgement fetch completed')), {{once:true}});
        movie.addEventListener('waiting', () => {{
            const held = movie.currentTime;
            console.log('waiting at:' + held.toFixed(2));
            fetch('/buffer-refill').then(() => console.log('waiting fetch completed'));
            audio.appendBuffer(shifted('{audio}', 292));
            setTimeout(() => {{
                console.log('held:' + [movie.currentTime === held, movie.paused,
                    movie.seeking, movie.ended, movie.readyState].join(':'));
                video.appendBuffer(shifted('{video}', 271));
            }}, 350);
        }}, {{once:true}});
        movie.addEventListener('timeupdate', () => {{
            if (!resumed && movie.currentTime > 271.25) {{
                resumed = true;
                console.log('resumed after video append');
            }}
        }});
        source.addEventListener('sourceopen', () => {{
            video = source.addSourceBuffer('video/mp4; codecs="avc1.42E01E"');
            audio = source.addSourceBuffer('audio/mp4; codecs="mp4a.40.2"');
            source.duration = 600;
            audio.addEventListener('updateend', () => {{
                if (++audioChunks < 22) audio.appendBuffer(shifted('{audio}', 270 + audioChunks));
                else if (audioChunks === 22) movie.play();
            }});
            video.appendBuffer(shifted('{video}', 270));
            audio.appendBuffer(shifted('{audio}', 270));
        }});
        movie.src = URL.createObjectURL(source);
    </script>"#
    )
    .into_bytes();
    session
        .load_document(
            document_start(document, html.len()),
            empty_document_state(),
            html,
        )
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(15);
    let mut evidence = std::collections::BTreeSet::new();
    let mut requests_seen = std::collections::BTreeSet::new();
    let (mut waited, mut held, mut resumed, mut pixels, mut native_paused) =
        (false, false, false, false, false);
    while Instant::now() < deadline
        && !(waited && held && resumed && pixels && native_paused && requests_seen.len() == 2)
    {
        let runtime = match session.wait_for_event(Duration::from_secs(3)).unwrap() {
            RendererEvent::Presentation(presentation) => {
                session
                    .acknowledge_presentation(PresentationAcknowledgement {
                        document,
                        revision: presentation.revision,
                        presented: true,
                        controls_applied: true,
                    })
                    .unwrap();
                presentation.runtime
            }
            RendererEvent::RuntimeUpdate(update) => update.runtime,
            RendererEvent::VideoFrame(frame) => {
                pixels |= held && !frame.pixels.is_empty();
                continue;
            }
            RendererEvent::Diagnostic { .. } => continue,
            RendererEvent::FetchBatch {
                document: actual,
                requests,
            } => {
                assert_eq!(actual, document);
                for request in requests {
                    let url = request.head.url;
                    assert!(url.ends_with("/playing-ack") || url.ends_with("/buffer-refill"));
                    requests_seen.insert(url.clone());
                    let sink = session.fetch_response_sink(document);
                    sink.start(FetchResponseHead {
                        request_id: request.head.request_id,
                        result: FetchResponseResult::Success {
                            response_type: FetchResponseType::Basic,
                            urls: vec![url],
                            status: 200,
                            headers: vec![("content-length".into(), "0".into())],
                        },
                    })
                    .unwrap();
                    sink.end(request.head.request_id, 0).unwrap();
                }
                continue;
            }
            event => panic!("unexpected starvation event: {event:?}"),
        };
        evidence.extend(runtime.console.iter().cloned());
        assert!(
            !runtime
                .console
                .iter()
                .any(|line| line.contains("unexpected")),
            "{:?}",
            runtime.console
        );
        waited |= runtime
            .console
            .iter()
            .any(|line| line.contains("waiting at:271.10"));
        held |= runtime
            .console
            .iter()
            .any(|line| line.contains("held:true:false:false:false:2"));
        resumed |= runtime
            .console
            .iter()
            .any(|line| line.contains("resumed after video append"));
        native_paused |= waited
            && runtime.media.as_ref().is_some_and(|media| {
                !media.playing && !media.ended && media.current_time_100ns < 2_712_500_000
            });
        run_scheduled_renderer_timer(&session, document, Some(20_000));
    }
    session.shutdown().unwrap();
    assert!(
        waited && held && resumed && pixels && native_paused && requests_seen.len() == 2,
        "waited={waited} held={held} resumed={resumed} pixels={pixels} native_paused={native_paused} requests={requests_seen:?}: {evidence:?}"
    );
}
