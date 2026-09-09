use super::*;
use std::time::Instant;

#[test]
fn unbuffered_seek_to_seven_minutes_resumes_after_separate_track_appends() {
    run_seek(false);
}

#[test]
fn loading_an_unrelated_audio_element_does_not_stop_seeked_video() {
    run_seek(true);
}

fn run_seek(reset_other: bool) {
    let _serial = SERIAL.lock().unwrap_or_else(|error| error.into_inner());
    let mut launch = options();
    launch.enable_media = true;
    let mut session = RendererSession::launch(launch).unwrap();
    let document = better_web_browser::renderer_protocol::DocumentId::new(197).unwrap();
    let video = include_str!("../../fixtures/media/test-1s-video-fragmented.mp4.base64")
        .split_whitespace()
        .collect::<String>();
    let audio = include_str!("../../fixtures/media/test-1s-audio-fragmented.mp4.base64")
        .split_whitespace()
        .collect::<String>();
    let html = format!(r#"<!doctype html><video id="movie" muted width="320" height="240"></video><script>
        const bytes = text => Uint8Array.from(atob(text), c => c.charCodeAt(0));
        function shifted(text) {{
            const data = bytes(text), view = new DataView(data.buffer);
            const tag = name => {{
                for (let i = 0; i < data.length - 4; i++)
                    if ([...name].every((c,n) => data[i+n] === c.charCodeAt(0))) return i;
                throw new Error('fixture box missing');
            }};
            const scale = view.getUint32(tag('mdhd') + 16);
            view.setBigUint64(tag('tfdt') + 8, BigInt(scale) * 420n);
            return data;
        }}
        const source = new MediaSource();
        let video, audio;
        movie.addEventListener('error', () => console.log('unexpected media error'));
        movie.addEventListener('loadeddata', () => {{ movie.currentTime = 420.5; movie.play(); }}, {{once:true}});
        movie.addEventListener('seeking', () => {{
            setTimeout(() => video.appendBuffer(shifted('{video}')), 50);
        }}, {{once:true}});
        movie.addEventListener('seeked', () => console.log('seek complete:' + movie.currentTime.toFixed(1)));
        let resetOther = {reset_other};
        movie.addEventListener('playing', () => {{
            if (resetOther) {{
                resetOther = false;
                const other = document.createElement('audio');
                document.body.appendChild(other);
                other.load();
            }}
        }});
        movie.addEventListener('timeupdate', () => {{
            if (!movie.seeking && movie.currentTime > 420.85) console.log('clock resumed at seven minutes');
        }});
        source.addEventListener('sourceopen', () => {{
            video = source.addSourceBuffer('video/mp4; codecs="avc1.42E01E"');
            audio = source.addSourceBuffer('audio/mp4; codecs="mp4a.40.2"');
            source.duration = 600;
            let updates = 0;
            video.addEventListener('updateend', () => {{
                if (++updates === 2) {{
                    console.log('waiting for audio:' + movie.currentTime + ':' + movie.seeking + ':' + movie.readyState);
                    setTimeout(() => audio.appendBuffer(shifted('{audio}')), 100);
                }}
            }});
            video.appendBuffer(bytes('{video}'));
            audio.appendBuffer(bytes('{audio}'));
        }});
        movie.src = URL.createObjectURL(source);
    </script>"#).into_bytes();
    session
        .load_document(
            document_start(document, html.len()),
            empty_document_state(),
            html,
        )
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(20);
    let (mut waited, mut sought, mut resumed, mut late_frame) = (false, false, false, false);
    while Instant::now() < deadline && !(waited && sought && resumed && late_frame) {
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
                late_frame |= sought && !frame.pixels.is_empty();
                continue;
            }
            RendererEvent::Diagnostic { .. } => continue,
            event => panic!("unexpected seek event: {event:?}"),
        };
        assert!(
            !runtime
                .console
                .iter()
                .any(|line| line.contains("unexpected media error")),
            "{:?}",
            runtime.console
        );
        waited |= runtime
            .console
            .iter()
            .any(|line| line.contains("waiting for audio:420.5:true:1"));
        sought |= runtime
            .console
            .iter()
            .any(|line| line.contains("seek complete:420.5"));
        resumed |= runtime
            .console
            .iter()
            .any(|line| line.contains("clock resumed at seven minutes"));
        run_scheduled_renderer_timer(&session, document, Some(20_000));
    }
    session.shutdown().unwrap();
    assert!(
        waited && sought && resumed && late_frame,
        "waited={waited} sought={sought} resumed={resumed} video={late_frame}"
    );
}
