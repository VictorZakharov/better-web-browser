use super::*;
use std::time::Instant;

#[test]
fn a_late_video_decode_error_does_not_stop_the_document() {
    late_decode_failure(true);
}

#[test]
fn a_video_presenter_decode_error_does_not_stop_the_document() {
    late_decode_failure(false);
}

fn late_decode_failure(seek: bool) {
    let _serial = SERIAL.lock().unwrap_or_else(|error| error.into_inner());
    let mut launch = options();
    launch.enable_media = true;
    let mut session = RendererSession::launch(launch).unwrap();
    let document = better_web_browser::renderer_protocol::DocumentId::new(196).unwrap();
    let video = include_str!("../../fixtures/media/test-1s-video-fragmented.mp4.base64")
        .split_whitespace()
        .collect::<String>();
    let audio = include_str!("../../fixtures/media/test-1s-audio-fragmented.mp4.base64")
        .split_whitespace()
        .collect::<String>();
    let html = format!(r#"<!doctype html><video id="movie" muted></video><script>
        const movie = document.getElementById('movie');
        const source = new MediaSource();
        const bytes = text => Uint8Array.from(atob(text), c => c.charCodeAt(0));
        const shift = data => {{
            const tag = name => {{
                for (let i = 0; i < data.length - 4; i++)
                    if ([...name].every((c,n) => data[i+n] === c.charCodeAt(0))) return i;
                throw new Error('fixture box missing');
            }};
            const view = new DataView(data.buffer);
            view.setBigUint64(tag('tfdt') + 8, BigInt(view.getUint32(tag('mdhd') + 16)) * 2n);
            return data;
        }};
        movie.addEventListener('error', () => {{
            console.log('decode-error:' + movie.error.code + ':' + movie.networkState);
            setTimeout(() => {{
                console.log('document survived decode error');
                movie.load();
                const replacement = new MediaSource();
                movie.addEventListener('loadeddata', () => console.log('replacement decoded'), {{once:true}});
                replacement.addEventListener('sourceopen', () => {{
                    replacement.addSourceBuffer('video/mp4; codecs="avc1.42E01E"').appendBuffer(bytes('{video}'));
                    replacement.addSourceBuffer('audio/mp4; codecs="mp4a.40.2"').appendBuffer(bytes('{audio}'));
                }}, {{once:true}});
                movie.src = URL.createObjectURL(replacement);
            }}, 10);
        }});
        source.addEventListener('sourceopen', () => {{
            const video = source.addSourceBuffer('video/mp4; codecs="avc1.42E01E"');
            const audio = source.addSourceBuffer('audio/mp4; codecs="mp4a.40.2"');
            source.duration = 4;
            let phase = 0;
            video.addEventListener('updateend', () => {{
                if (++phase === 1) {{
                    const bad = bytes('{video}');
                    if ({seek}) shift(bad);
                    for (let i = 0; i + 4 < bad.length; i++) {{
                        if (bad[i] === 109 && bad[i+1] === 100 && bad[i+2] === 97 && bad[i+3] === 116) {{
                            bad.fill(0, i+4); break;
                        }}
                    }}
                    video.appendBuffer(bad);
                    if ({seek}) audio.appendBuffer(shift(bytes('{audio}')));
                }} else if (phase === 2) {{
                    if ({seek}) movie.currentTime = 2.5;
                    else movie.play();
                }}
            }});
            const initial = bytes('{video}');
            if (!{seek}) {{
                // Keep the audio clock running beyond the first five valid video samples.
                // The later corrupt segment must be reached by the presenter, not seek().
                const view = new DataView(initial.buffer);
                for (let i = 0; i + 12 < initial.length; i++) {{
                    if (initial[i] === 116 && initial[i+1] === 114 && initial[i+2] === 117 && initial[i+3] === 110) {{
                        view.setUint32(i+8, 5); break;
                    }}
                }}
            }}
            video.appendBuffer(initial);
            audio.appendBuffer(bytes('{audio}'));
        }}, {{once:true}});
        movie.src = URL.createObjectURL(source);
        </script>"#).into_bytes();
    session
        .load_document(
            document_start(document, html.len()),
            empty_document_state(),
            html,
        )
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(15);
    let mut error_reported = false;
    let mut survived = false;
    let mut replaced = false;
    while Instant::now() < deadline && !replaced {
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
            RendererEvent::Diagnostic { .. } | RendererEvent::VideoFrame(_) => continue,
            event => panic!("media failure escaped its element: {event:?}"),
        };
        error_reported |= runtime
            .console
            .iter()
            .any(|line| line.contains("decode-error:3:1"));
        survived |= runtime
            .console
            .iter()
            .any(|line| line.contains("document survived decode error"));
        run_scheduled_renderer_timer(&session, document, Some(20_000));
        replaced = runtime
            .console
            .iter()
            .any(|line| line.contains("replacement decoded"));
    }
    assert!(error_reported, "missing HTMLMediaElement decode error");
    assert!(
        survived,
        "document timer did not run after the media failure"
    );
    assert!(
        replaced,
        "failed source could not be replaced on the same worker"
    );
    session.shutdown().unwrap();
}
