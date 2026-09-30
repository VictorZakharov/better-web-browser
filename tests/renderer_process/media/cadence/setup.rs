//! Finite, sustained playback setup and bounded readiness evidence for cadence tests.

use super::*;
use better_web_browser::renderer_protocol::{DocumentId, RuntimeReport};
use std::collections::VecDeque;

pub(super) fn start_playback(session: &RendererSession, document: DocumentId, busy_ms: u32) -> u64 {
    let body = sustained_source(busy_ms).into_bytes();
    session
        .load_document(
            document_start(document, body.len()),
            empty_document_state(),
            body,
        )
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut evidence = VecDeque::new();
    let mut frame = None;
    let (mut buffered, mut playing) = (false, false);
    let mut native_playing = false;
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        assert!(
            !remaining.is_zero(),
            "video producer did not start: {evidence:?}"
        );
        let event = session.wait_for_event(remaining).unwrap_or_else(|error| {
            panic!("video startup exceeded its 10 s deadline: {error}; {evidence:?}")
        });
        let (runtime, next) = match event {
            RendererEvent::VideoFrame(update) if update.identity.document == document => {
                assert!(!update.pixels.is_empty());
                let current_frame = update.identity.frame;
                frame = Some(current_frame);
                if buffered && playing && native_playing {
                    return current_frame;
                }
                continue;
            }
            RendererEvent::Presentation(presentation) if presentation.document == document => {
                acknowledge(session, &presentation);
                (presentation.runtime, presentation.next_timer_micros)
            }
            RendererEvent::RuntimeUpdate(update) if update.document == document => {
                (update.runtime, update.next_timer_micros)
            }
            RendererEvent::Diagnostic { code, text } => {
                remember(&mut evidence, format!("diagnostic {code}: {text}"));
                continue;
            }
            event => panic!("unexpected video startup event: {event:?}; {evidence:?}"),
        };
        for line in runtime.console.iter().chain(&runtime.diagnostics) {
            remember(&mut evidence, line.clone());
        }
        if let Some(media) = &runtime.media {
            remember(&mut evidence, format!("media: {media:?}"));
            native_playing = media.active && media.playing && !media.ended;
            if native_playing {
                assert!(
                    media.duration_100ns >= 70_000_000,
                    "sustained source was truncated: {media:?}; {evidence:?}"
                );
            }
            assert!(!media.ended, "video ended before readiness: {evidence:?}");
        }
        assert_runtime_healthy(&runtime);
        buffered |= runtime
            .console
            .iter()
            .any(|line| line.contains("__CADENCE_BUFFERED__"));
        playing |= runtime
            .console
            .iter()
            .any(|line| line.contains("__CADENCE_PLAYING__"));
        if buffered
            && playing
            && native_playing
            && let Some(frame) = frame
        {
            return frame;
        }
        // Follow the advertised task, not arbitrary logical fast-forwarding.
        // Yield real time for codec/producer work when the next poll is in the future.
        if let Some(delay) = next {
            std::thread::sleep(
                Duration::from_micros(delay.min(20_000))
                    .min(deadline.saturating_duration_since(Instant::now())),
            );
            run_scheduled_renderer_timer(session, document, Some(delay));
        }
    }
}

pub(super) fn assert_runtime_healthy(runtime: &RuntimeReport) {
    assert!(
        runtime.errors.is_empty(),
        "video script errors: {:?}",
        runtime.errors
    );
    assert!(
        !runtime
            .console
            .iter()
            .any(|line| line.contains("__CADENCE_ERROR__")),
        "video setup failed: {:?}",
        runtime.console
    );
    assert!(
        runtime
            .media
            .as_ref()
            .is_none_or(|media| media.failure.is_none()),
        "video worker failed: {:?}",
        runtime.media
    );
}

fn remember(evidence: &mut VecDeque<String>, line: String) {
    if evidence.len() == 24 {
        evidence.pop_front();
    }
    evidence.push_back(line.chars().take(512).collect());
}

fn sustained_source(busy_ms: u32) -> String {
    let video = include_str!("../../../fixtures/media/test-1s-video-fragmented.mp4.base64")
        .split_whitespace()
        .collect::<String>();
    let audio = include_str!("../../../fixtures/media/test-1s-audio-fragmented.mp4.base64")
        .split_whitespace()
        .collect::<String>();
    // Reuse the licensed fixture and the timestamp-shifted append contract from
    // starvation.rs. Eight finite seconds leave real playback after cold setup;
    // neither the document watchdog nor the 10 s startup deadline is extended.
    format!(
        r#"<!doctype html><video id="movie" muted width="320" height="240"></video><script>
        function shifted(text, seconds) {{
            const bytes = Uint8Array.from(atob(text), c => c.charCodeAt(0));
            const view = new DataView(bytes.buffer);
            const tag = name => {{
                for (let i = 0; i < bytes.length - 4; i++)
                    if ([...name].every((c, n) => bytes[i + n] === c.charCodeAt(0))) return i;
                throw new Error('fixture box missing: ' + name);
            }};
            const scale = view.getUint32(tag('mdhd') + 16);
            view.setBigUint64(tag('tfdt') + 8, BigInt(scale) * BigInt(seconds));
            return bytes;
        }}
        const source = new MediaSource();
        movie.addEventListener('error', () => console.log('__CADENCE_ERROR__:media'));
        source.addEventListener('sourceopen', () => {{
            console.log('__CADENCE_SOURCE_OPEN__');
            const video = source.addSourceBuffer('video/mp4; codecs="avc1.42E01E"');
            const audio = source.addSourceBuffer('audio/mp4; codecs="mp4a.40.2"');
            source.duration = 8;
            let segment = 0, completed = 0;
            const append = () => {{
                completed = 0;
                video.appendBuffer(shifted('{video}', segment));
                audio.appendBuffer(shifted('{audio}', segment));
                segment++;
            }};
            const updated = () => {{
                if (++completed !== 2) return;
                if (segment < 8) append();
                else {{
                    console.log('__CADENCE_BUFFERED__');
                    source.endOfStream();
                }}
            }};
            video.addEventListener('updateend', updated);
            audio.addEventListener('updateend', updated);
            append();
        }}, {{once:true}});
        source.addEventListener('sourceended', () => movie.play().then(
            () => console.log('__CADENCE_PLAYING__'),
            error => console.log('__CADENCE_ERROR__:' + error.name)
        ), {{once:true}});
        document.addEventListener('keydown', () => {{
            const start = performance.now();
            while (performance.now() - start < {busy_ms}) {{}}
            console.log('__BUSY_CALLBACK_COMPLETED__');
        }}, {{once:true}});
        movie.src = URL.createObjectURL(source);
        </script>"#
    )
}
