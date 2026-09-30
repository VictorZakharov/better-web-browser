//! Native fallback targeting uses current document membership and retained geometry.
use super::*;
use better_web_browser::renderer_protocol::{DocumentId, WheelInput};

fn native_wheel(document: DocumentId, sequence: u64, x: f32, y: f32) -> DocumentInput {
    DocumentInput::Wheel(WheelInput {
        document,
        sequence,
        x,
        y,
        delta_x: 0.0,
        delta_y: 75.0,
        viewport_y: 0.0,
        modifiers: InputModifiers::default(),
        target: None,
    })
}

fn listener_ready(
    session: &RendererSession,
    initial: better_web_browser::renderer_protocol::RendererPresentation,
) -> better_web_browser::renderer_protocol::RendererPresentation {
    if presentation_text(&initial).contains("listener-ready") {
        return initial;
    }
    let document = initial.document;
    // A long parser task may publish paint before the trailing script executes.
    // Follow only its advertised immediate work, just as the browser shell does.
    acknowledge(session, &initial);
    pump_ready_task(session, document, initial.next_timer_micros);
    for _ in 0..16 {
        match session.wait_for_event(Duration::from_secs(3)).unwrap() {
            RendererEvent::Presentation(presentation) if presentation.document == document => {
                assert!(
                    presentation.runtime.errors.is_empty(),
                    "{:?}",
                    presentation.runtime.errors
                );
                assert!(!presentation.runtime.runtime_stopped);
                if presentation_text(&presentation).contains("listener-ready") {
                    return *presentation;
                }
                acknowledge(session, &presentation);
                pump_ready_task(session, document, presentation.next_timer_micros);
            }
            RendererEvent::RuntimeUpdate(update) if update.document == document => {
                assert!(
                    update.runtime.errors.is_empty(),
                    "{:?}",
                    update.runtime.errors
                );
                assert!(!update.runtime.runtime_stopped);
                pump_ready_task(session, document, update.next_timer_micros);
            }
            RendererEvent::Diagnostic { .. } => {}
            event => panic!("unexpected listener-readiness event: {event:?}"),
        }
    }
    panic!("parser did not install the owned wheel listener");
}

#[test]
fn native_hit_target_index_preserves_trusted_cancelled_wheels_on_a_long_plain_document() {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut session = RendererSession::launch(options()).expect("hidden renderer");
    let html = format!(
        r#"<!doctype html><style>
        body{{margin:0}} main>div{{height:10px;width:100px;background:red}}
        #status{{position:absolute;left:200px;top:0}}
        </style><main><div id=first></div>{}</main><p id=status>loading</p><script>
        document.querySelector('#first').addEventListener('wheel',event=>{{
            event.preventDefault();
            document.querySelector('#status').textContent='first:'+event.target.id+':'+event.isTrusted;
        }},{{passive:false}});
        document.querySelector('#status').textContent='listener-ready';</script>"#,
        "<div></div>".repeat(1_999)
    );
    let initial = listener_ready(
        &session,
        load_html_document_with_selectors(&session, 890, &html, vec!["#first".into()]),
    );
    // Present the verdict in the viewport, not below the 20,000-pixel fixture:
    // retained presentation deliberately culls offscreen paint items.
    assert!(presentation_text(&initial).contains("listener-ready"));
    assert!(
        initial.runtime.errors.is_empty(),
        "{:?}",
        initial.runtime.errors
    );
    let first_rect = initial.page_diagnostics.selectors[0].matches[0]
        .layout_rect
        .expect("first plain block geometry");
    assert_eq!((first_rect.width, first_rect.height), (100.0, 10.0));
    acknowledge(&session, &initial);
    session
        .send_input(native_wheel(
            initial.document,
            1,
            first_rect.x + 50.0,
            first_rect.y + 5.0,
        ))
        .unwrap();
    let next = wait_for_text(&session, initial.document, "first:first:true");
    assert!(next.runtime.errors.is_empty(), "{:?}", next.runtime.errors);
    assert_eq!(next.runtime.viewport_wheel_delta_y, 0.0);
    assert!(matches!(next.runtime.wheel_acknowledgements.as_slice(),
        [ack] if ack.sequence == 1 && ack.decision == better_web_browser::renderer_protocol::WheelDecision::Cancelled));
    session.shutdown().expect("shutdown hidden renderer");
}

#[test]
fn native_hit_target_index_tracks_adoption_removal_reinsertion_closed_shadow_and_retirement() {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut session = RendererSession::launch(options()).expect("hidden renderer");
    let initial = load_html_document(
        &session,
        891,
        r#"<!doctype html><style>
        body{margin:0}#host{position:absolute;left:250px;top:0;width:180px;height:100px}
        #status{position:absolute;top:200px}</style>
        <div id=host></div><p id=status>ready</p><script>
        const seen=[], status=document.querySelector('#status');
        const record=event=>{event.preventDefault(); seen.push(event.target.id+':'+event.isTrusted); status.textContent=seen.join('|');};
        const foreign=document.implementation.createHTMLDocument('foreign');
        const adopted=foreign.createElement('div'); adopted.id='adopted';
        adopted.style.cssText='position:absolute;left:0;top:0;width:180px;height:100px;background:red';
        document.body.appendChild(document.adoptNode(adopted));
        let count=0;
        adopted.addEventListener('wheel',event=>{
            record(event);
            if (++count===1) {
                const replacement=document.createElement('div'); replacement.id='replacement';
                replacement.style.cssText=adopted.style.cssText;
                replacement.addEventListener('wheel',event=>{record(event);replacement.replaceWith(adopted);},{passive:false});
                adopted.replaceWith(replacement);
            }
        },{passive:false});
        const shadow=document.querySelector('#host').attachShadow({mode:'closed'});
        shadow.innerHTML='<div id="private" style="width:180px;height:100px;background:blue"></div>';
        shadow.querySelector('#private').addEventListener('wheel',record,{passive:false});
        </script>"#,
    );
    acknowledge(&session, &initial);
    for (sequence, x, expected) in [
        (1, 50.0, "adopted:true"),
        (2, 50.0, "adopted:true|replacement:true"),
        (3, 50.0, "adopted:true|replacement:true|adopted:true"),
        (
            4,
            300.0,
            "adopted:true|replacement:true|adopted:true|private:true",
        ),
    ] {
        session
            .send_input(native_wheel(initial.document, sequence, x, 50.0))
            .unwrap();
        let next = wait_for_text(&session, initial.document, expected);
        assert!(next.runtime.errors.is_empty(), "{:?}", next.runtime.errors);
        assert_eq!(next.runtime.viewport_wheel_delta_y, 0.0);
        acknowledge(&session, &next);
    }
    session.cancel_document(initial.document).unwrap();
    let replacement = load_html_document(
        &session,
        892,
        r#"<!doctype html><style>
        body{margin:0}#fresh{width:180px;height:100px;background:green}</style>
        <div id=fresh></div><p id=status>replacement-ready</p><script>
        document.querySelector('#fresh').addEventListener('wheel',event=>{
            event.preventDefault();document.querySelector('#status').textContent='fresh:'+event.target.id+':'+event.isTrusted;
        },{passive:false});</script>"#,
    );
    acknowledge(&session, &replacement);
    session
        .send_input(native_wheel(initial.document, 5, 50.0, 50.0))
        .unwrap();
    session
        .send_input(native_wheel(replacement.document, 1, 50.0, 50.0))
        .unwrap();
    let next = wait_for_text(&session, replacement.document, "fresh:fresh:true");
    assert!(!presentation_text(&next).contains("adopted"));
    assert!(next.runtime.errors.is_empty(), "{:?}", next.runtime.errors);
    assert_eq!(next.runtime.wheel_acknowledgements.len(), 1);
    assert_eq!(next.runtime.wheel_acknowledgements[0].sequence, 1);
    session.shutdown().expect("shutdown hidden renderer");
}
