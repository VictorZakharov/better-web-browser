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
