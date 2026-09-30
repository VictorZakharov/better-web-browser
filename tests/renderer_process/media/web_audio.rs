//! The page graph must reach the contained media worker, not merely expose AudioContext.

use super::*;
use better_web_browser::renderer_protocol::{
    DocumentId, DocumentInput, InputModifiers, KeyPhase, KeyboardInput,
};
use std::time::Instant;

#[test]
fn live_audio_context_streams_after_trusted_input_and_closes() {
    let _serial = SERIAL.lock().unwrap_or_else(|error| error.into_inner());
    let mut launch = options();
    launch.enable_media = true;
    // options() sets test mode and silent audio: this exercises IPC without making sound.
    assert!(launch.test_mode && launch.silent_audio);
    let mut session =
        RendererSession::launch(launch).expect("launch hidden renderer and media worker");
    let document = DocumentId::new(219).unwrap();
    let html = r#"<!doctype html><title>live Web Audio</title>
        <output id="state">waiting</output><script>
        const context = new AudioContext({sampleRate: 48000});
        const source = context.createOscillator();
        const gain = context.createGain();
        source.frequency.value = 440;
        gain.gain.value = 0.01;
        source.connect(gain).connect(context.destination);
        source.start();
        document.addEventListener('keydown', () => {
            context.resume().then(() => {
                state.textContent = 'running';
                setTimeout(() => {
                    const advanced = context.currentTime > 0;
                    context.close().then(() => {
                        state.textContent = 'closed:' + advanced + ':' + context.state;
                    }).catch(error => state.textContent = 'close-error:' + error.name);
                }, 80);
            }).catch(error => state.textContent = 'resume-error:' + error.name);
        }, {once: true});
        </script>"#;
    let initial = load_html_document(&session, document.get(), html);
    session
        .acknowledge_presentation(PresentationAcknowledgement {
            document,
            revision: initial.revision,
            presented: true,
            controls_applied: true,
        })
        .unwrap();
    session
        .send_input(DocumentInput::Keyboard(KeyboardInput {
            document,
            sequence: 1,
            phase: KeyPhase::Down,
            key: "a".into(),
            code: "KeyA".into(),
            repeat: false,
            modifiers: InputModifiers::default(),
            target: None,
        }))
        .unwrap();

    let deadline = Instant::now() + Duration::from_secs(15);
    let mut last_text = String::new();
    let mut last_console = Vec::new();
    let mut advanced_close_timer = false;
    let observed = loop {
        assert!(
            Instant::now() < deadline,
            "live graph did not close after starting; last text={last_text:?}, console={last_console:?}"
        );
        match session.wait_for_event(Duration::from_secs(5)).unwrap() {
            RendererEvent::Presentation(presentation) if presentation.document == document => {
                let text = presentation
                    .layout
                    .items
                    .iter()
                    .filter_map(|item| {
                        let DisplayItem::Text { text, .. } = item else {
                            return None;
                        };
                        Some(text.as_str())
                    })
                    .collect::<String>();
                last_text = text.clone();
                last_console = presentation.runtime.console.clone();
                session
                    .acknowledge_presentation(PresentationAcknowledgement {
                        document,
                        revision: presentation.revision,
                        presented: true,
                        controls_applied: true,
                    })
                    .unwrap();
                run_scheduled_renderer_timer(&session, document, presentation.next_timer_micros);
                if text.contains("running") && !advanced_close_timer {
                    advanced_close_timer = true;
                    session
                        .advance_time(document, Duration::from_millis(100), 64)
                        .expect("advance the script close timer after the graph starts");
                }
                if text.contains("closed:true:closed") {
                    break text;
                }
                assert!(!text.contains("-error:"), "{text}");
            }
            RendererEvent::RuntimeUpdate(update) if update.document == document => {
                last_console = update.runtime.console.clone();
                run_scheduled_renderer_timer(&session, document, update.next_timer_micros);
            }
            RendererEvent::Diagnostic { .. } => {}
            event => panic!("unexpected live-audio event: {event:?}"),
        }
    };
    assert!(observed.contains("closed:true:closed"));
    session
        .shutdown()
        .expect("shutdown contained renderer and media worker");
}

#[test]
fn navigation_retires_compressed_audio_decode_callbacks_with_the_old_document() {
    let _serial = SERIAL.lock().unwrap_or_else(|error| error.into_inner());
    let mut session = RendererSession::launch(options()).expect("launch hidden decode renderer");
    for (index, fixture) in [
        include_str!("../../fixtures/media/test-0.4s-tone.aac.base64"),
        include_str!("../../fixtures/media/test-0.4s-tone.webm.base64"),
        include_str!("../../fixtures/media/test-0.4s-tone.oga.base64"),
    ]
    .into_iter()
    .enumerate()
    {
        let old = DocumentId::new(221 + index as u64 * 2).unwrap();
        let replacement = DocumentId::new(old.get() + 1).unwrap();
        let html = r#"<!doctype html><output id="state">decode started</output><script>
            const context = new OfflineAudioContext(1, 128, 48000);
            const input = Uint8Array.from(atob('__FIXTURE__'),
                character => character.charCodeAt(0)).buffer;
            context.decodeAudioData(input).then(() => {
                state.textContent = 'old decode completed';
                console.log('old decode callback');
            }, error => console.log('old decode rejected:' + error.name));
            if (input.byteLength !== 0) throw Error('input not detached');
        </script>"#
            .replace("__FIXTURE__", &fixture.lines().collect::<String>());
        let initial = load_html_document(&session, old.get(), &html);
        assert!(
            initial.runtime.errors.is_empty(),
            "{:?}",
            initial.runtime.errors
        );
        // Retire without advancing media completion tasks. A worker may finish
        // either side of this cancellation; its result still belongs to `old`.
        session.cancel_document(old).unwrap();
        let html = "<!doctype html><output>replacement</output><script>\
            setTimeout(() => console.log('replacement checkpoint'), 30);</script>";
        session
            .load_document(
                document_start(replacement, html.len()),
                empty_document_state(),
                html.as_bytes().to_vec(),
            )
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut checkpoint = false;
        while !checkpoint {
            assert!(
                Instant::now() < deadline,
                "replacement decode lifecycle did not settle"
            );
            match session.wait_for_event(Duration::from_secs(3)).unwrap() {
                RendererEvent::Presentation(presentation)
                    if presentation.document == replacement =>
                {
                    assert!(
                        presentation.runtime.errors.is_empty(),
                        "{:?}",
                        presentation.runtime.errors
                    );
                    assert!(
                        !presentation
                            .runtime
                            .console
                            .iter()
                            .any(|line| line.contains("old decode"))
                    );
                    checkpoint |= presentation
                        .runtime
                        .console
                        .iter()
                        .any(|line| line.contains("replacement checkpoint"));
                    session
                        .acknowledge_presentation(PresentationAcknowledgement {
                            document: replacement,
                            revision: presentation.revision,
                            presented: true,
                            controls_applied: true,
                        })
                        .unwrap();
                    run_scheduled_renderer_timer(
                        &session,
                        replacement,
                        presentation.next_timer_micros,
                    );
                }
                RendererEvent::RuntimeUpdate(update) if update.document == replacement => {
                    assert!(
                        update.runtime.errors.is_empty(),
                        "{:?}",
                        update.runtime.errors
                    );
                    assert!(
                        !update
                            .runtime
                            .console
                            .iter()
                            .any(|line| line.contains("old decode"))
                    );
                    checkpoint |= update
                        .runtime
                        .console
                        .iter()
                        .any(|line| line.contains("replacement checkpoint"));
                    run_scheduled_renderer_timer(&session, replacement, update.next_timer_micros);
                }
                // Already-enqueued old-document envelopes remain tagged with
                // their owner and cannot run callbacks in the replacement realm.
                RendererEvent::Presentation(_)
                | RendererEvent::RuntimeUpdate(_)
                | RendererEvent::Diagnostic { .. } => {}
                event => panic!("unexpected decode retirement event: {event:?}"),
            }
        }
        session.cancel_document(replacement).unwrap();
    }
    session.shutdown().expect("shutdown hidden decode renderer");
}
