use super::*;

#[test]
fn contained_renderer_opus_mono_and_stereo_sources_play_and_seek_without_video() {
    for (document, fixture, path, mime, response_type) in [
        (
            230,
            include_str!("../../../fixtures/media/test-0.4s-opus.ogg.base64"),
            "/tone.opus",
            "audio/ogg; codecs=\"opus\"",
            "audio/ogg",
        ),
        (
            231,
            include_str!("../../../fixtures/media/test-0.4s-opus-stereo.ogg.base64"),
            "/stereo.opus",
            "audio/ogg; codecs=\"opus\"",
            "audio/ogg",
        ),
        (
            234,
            include_str!("../../../fixtures/media/test-0.4s-opus.webm.base64"),
            "/tone.webm",
            "audio/webm; codecs=\"opus\"",
            "audio/webm",
        ),
    ] {
        verify_audio_source_with_seek(
            document,
            path,
            mime,
            response_type,
            "Opus",
            decode_base64(fixture),
            "0.2",
        );
    }
}

#[test]
fn contained_renderer_corrupt_or_truncated_opus_rejects_pending_play() {
    let bytes = decode_base64(include_str!(
        "../../../fixtures/media/test-0.4s-opus.ogg.base64"
    ));
    let mut corrupt = bytes.clone();
    *corrupt.last_mut().unwrap() ^= 1;
    for damaged in [bytes[..bytes.len() - 1].to_vec(), corrupt] {
        verify_audio_failure(200, damaged, 4, "audio/ogg");
    }
}

#[test]
fn contained_renderer_decodes_opus_audio_buffers_with_real_resampled_pcm_and_async_callbacks() {
    let _serial = SERIAL.lock().unwrap_or_else(|error| error.into_inner());
    let mut session =
        RendererSession::launch(options()).expect("launch hidden Opus decode renderer");
    for (id, fixture, channels, rate, frames) in [
        (
            232,
            include_str!("../../../fixtures/media/test-0.4s-opus.ogg.base64"),
            1,
            24_000,
            9_600,
        ),
        (
            233,
            include_str!("../../../fixtures/media/test-0.4s-opus-stereo.ogg.base64"),
            2,
            44_100,
            17_640,
        ),
        (
            235,
            include_str!("../../../fixtures/media/test-0.4s-opus.webm.base64"),
            1,
            96_000,
            38_400,
        ),
    ] {
        let document = better_web_browser::renderer_protocol::DocumentId::new(id).unwrap();
        let html = r#"<!doctype html><output id="state">waiting</output><script>
            const context = new OfflineAudioContext(__CHANNELS__, 128, __RATE__);
            const input = Uint8Array.from(atob('__FIXTURE__'), c => c.charCodeAt(0)).buffer;
            const events = [];
            const promise = context.decodeAudioData(input, buffer => {
                events.push('callback');
                if (buffer.sampleRate !== __RATE__ || buffer.length !== __FRAMES__ ||
                    buffer.numberOfChannels !== __CHANNELS__ ||
                    Math.abs(buffer.duration - 0.4) > 1e-9) throw Error('Opus metadata');
            });
            if (input.byteLength !== 0) throw Error('Opus input was not detached');
            events.push('returned');
            promise.then(buffer => {
                events.push('then');
                for (let index = 0; index < buffer.numberOfChannels; index++) {
                    const pcm = buffer.getChannelData(index);
                    if (!pcm.every(Number.isFinite) || !pcm.some(s => s > 0.02) ||
                        !pcm.some(s => s < -0.02)) throw Error('Opus PCM');
                }
                if (buffer.numberOfChannels === 2 &&
                    buffer.getChannelData(0).every((s, i) => s === buffer.getChannelData(1)[i]))
                    throw Error('Opus stereo channels were duplicated');
                if (events.join(',') !== 'returned,callback,then') throw Error('callback ordering');
                state.textContent = 'Opus decoded';
            }).catch(error => state.textContent = 'decode-error:' + error.name + ':' + error.message);
        </script>"#
            .replace("__FIXTURE__", &fixture.lines().collect::<String>())
            .replace("__CHANNELS__", &channels.to_string())
            .replace("__RATE__", &rate.to_string())
            .replace("__FRAMES__", &frames.to_string());
        let initial = load_html_document(&session, document.get(), &html);
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut presentation = initial;
        loop {
            assert!(Instant::now() < deadline, "Opus AudioBuffer did not settle");
            assert!(
                presentation.runtime.errors.is_empty(),
                "{:?}",
                presentation.runtime.errors
            );
            let visible = presentation
                .layout
                .items
                .iter()
                .filter_map(|item| {
                    if let DisplayItem::Text { text, .. } = item {
                        Some(text.as_str())
                    } else {
                        None
                    }
                })
                .collect::<String>();
            // Normal CSS whitespace creates separate paint atoms for each word.
            let done = visible.contains("Opus decoded");
            assert!(!visible.contains("decode-error:"), "{visible}");
            session
                .acknowledge_presentation(PresentationAcknowledgement {
                    document,
                    revision: presentation.revision,
                    presented: true,
                    controls_applied: true,
                })
                .unwrap();
            if done {
                break;
            }
            run_scheduled_renderer_timer(&session, document, presentation.next_timer_micros);
            presentation = loop {
                assert!(Instant::now() < deadline, "Opus decode renderer timed out");
                match session.wait_for_event(Duration::from_secs(3)).unwrap() {
                    RendererEvent::Presentation(next) if next.document == document => break *next,
                    RendererEvent::RuntimeUpdate(update) if update.document == document => {
                        assert!(
                            update.runtime.errors.is_empty(),
                            "{:?}",
                            update.runtime.errors
                        );
                        run_scheduled_renderer_timer(&session, document, update.next_timer_micros);
                    }
                    RendererEvent::Diagnostic { .. } => {}
                    event => panic!("unexpected Opus decode event: {event:?}"),
                }
            };
        }
        session.cancel_document(document).unwrap();
    }
    session
        .shutdown()
        .expect("shutdown hidden Opus decode renderer");
}
