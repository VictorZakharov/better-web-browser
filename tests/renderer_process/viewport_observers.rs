use super::support::*;
use better_web_browser::renderer_process::{RendererEvent, RendererSession};
use better_web_browser::renderer_protocol::{
    DocumentId, DocumentInput, InputModifiers, PointerButton, PointerInput, PointerPhase,
    PresentationAcknowledgement, RuntimeReport, ScrollInput, WheelInput,
};
use std::time::Duration;

fn next_report(
    session: &RendererSession,
    document: DocumentId,
) -> (bool, bool, Option<u64>, RuntimeReport) {
    loop {
        match session
            .wait_for_event(Duration::from_secs(3))
            .expect("renderer observer event")
        {
            RendererEvent::RuntimeUpdate(update) if update.document == document => {
                assert!(
                    update.runtime.errors.is_empty(),
                    "{:?}",
                    update.runtime.errors
                );
                return (
                    false,
                    update.clock_advanced,
                    update.next_timer_micros,
                    update.runtime,
                );
            }
            RendererEvent::Presentation(presentation) if presentation.document == document => {
                session
                    .acknowledge_presentation(PresentationAcknowledgement {
                        document,
                        revision: presentation.revision,
                        presented: true,
                        controls_applied: true,
                    })
                    .unwrap();
                assert!(
                    presentation.runtime.errors.is_empty(),
                    "{:?}",
                    presentation.runtime.errors
                );
                return (
                    true,
                    presentation.clock_advanced,
                    presentation.next_timer_micros,
                    presentation.runtime,
                );
            }
            RendererEvent::Diagnostic { .. }
            | RendererEvent::PointerCursor(_)
            | RendererEvent::ScrollCompleted { .. } => {}
            event => panic!("unexpected observer event: {event:?}"),
        }
    }
}

fn advance(session: &RendererSession, document: DocumentId) -> (bool, RuntimeReport) {
    session.advance_time(document, Duration::ZERO, 1).unwrap();
    let (painted, advanced, _, report) = next_report(session, document);
    assert!(advanced, "expected dedicated observer task completion");
    (painted, report)
}

#[test]
fn native_scroll_positions_share_one_pending_rendering_event() {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let session = RendererSession::launch(options()).expect("hidden renderer");
    let initial = load_html_document(
        &session,
        229,
        r#"<!doctype html><style>body{height:2000px}</style><script>
            addEventListener('scroll', () => console.log('scroll:' + scrollY));
        </script>"#,
    );
    for (sequence, y) in [(1, 10.0), (2, 20.0), (3, 30.0)] {
        session
            .send_input(DocumentInput::Scroll(ScrollInput {
                document: initial.document,
                sequence,
                x: 0.0,
                y,
            }))
            .unwrap();
        let (painted, advanced, next, report) = next_report(&session, initial.document);
        assert!(!painted && !advanced && report.console.is_empty());
        assert_eq!(next, Some(0));
    }
    let (_, rendered) = advance(&session, initial.document);
    assert_eq!(rendered.console, ["log: scroll:30"]);
    assert!(advance(&session, initial.document).1.console.is_empty());
}

#[test]
fn wheel_listener_sees_native_position_before_a_coalesced_scroll_event() {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let session = RendererSession::launch(options()).expect("hidden renderer");
    let initial = load_html_document(
        &session,
        230,
        r#"<!doctype html><style>body{height:2000px}</style><script>
            addEventListener('wheel', () => console.log('wheel:' + scrollY));
            addEventListener('scroll', () => console.log('scroll:' + scrollY));
        </script>"#,
    );
    session
        .send_input(DocumentInput::Wheel(WheelInput {
            document: initial.document,
            sequence: 2,
            x: 20.0,
            y: 50.0,
            delta_x: 0.0,
            delta_y: 0.0,
            viewport_y: 30.0,
            modifiers: InputModifiers::default(),
            target: None,
        }))
        .unwrap();
    let (_, _, next, wheel) = next_report(&session, initial.document);
    assert_eq!(wheel.console, ["log: wheel:30"]);
    assert_eq!(next, Some(0));
    let (_, scroll) = advance(&session, initial.document);
    assert_eq!(scroll.console, ["log: scroll:30"]);
}

#[test]
fn native_scroll_queues_observer_work_without_relayout_or_author_event_propagation() {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut session = RendererSession::launch(options()).expect("hidden renderer");
    let initial = load_html_document(
        &session,
        119,
        r#"<!doctype html>
        <style>body{margin:0} #target{position:absolute;top:1000px;width:100px;height:100px}</style>
        <div id=target></div><script>
            const target = document.getElementById('target');
            new IntersectionObserver(entries => {
                for (const e of entries) console.log('entry:' + e.isIntersecting + ':' + e.boundingClientRect.top);
            }).observe(target);
            document.addEventListener('scroll', event => {
                event.stopPropagation(); event.stopImmediatePropagation();
            });
            document.dispatchEvent(new Event('scroll', {bubbles:true}));
        </script>"#,
    );
    assert!(
        initial.runtime.console.is_empty(),
        "synthetic scroll delivered observations"
    );
    let (painted, first) = advance(&session, initial.document);
    assert!(!painted);
    assert_eq!(first.console, ["log: entry:false:1000"]);
    session
        .send_input(DocumentInput::Scroll(ScrollInput {
            document: initial.document,
            sequence: 1,
            x: 0.0,
            y: 600.0,
        }))
        .unwrap();
    let (painted, advanced, next, queued) = next_report(&session, initial.document);
    assert!(
        !painted && !advanced,
        "scroll should schedule, not run, the observer task"
    );
    assert_eq!(
        next,
        Some(0),
        "quiet scroll must wake the browser-owned clock"
    );
    assert!(!queued.render_requested);
    assert!(queued.console.is_empty());
    let (painted, scroll) = advance(&session, initial.document);
    assert!(!painted && !scroll.render_requested);
    assert!(
        scroll.console.is_empty(),
        "observer ran in the scroll update"
    );
    let (painted, visible) = advance(&session, initial.document);
    assert!(!painted && !visible.render_requested);
    assert_eq!(visible.console, ["log: entry:true:400"]);
    assert!(
        advance(&session, initial.document).1.console.is_empty(),
        "duplicate unchanged entry"
    );
    session.shutdown().unwrap();
}

#[test]
fn native_scroll_observer_task_uses_geometry_after_nested_promise_mutations() {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut session = RendererSession::launch(options()).expect("hidden renderer");
    let initial = load_html_document(
        &session,
        120,
        r#"<!doctype html>
        <style>body{margin:0} #target{position:absolute;top:1000px;width:100px;height:100px}</style>
        <div id=target></div><script>
            const target = document.getElementById('target');
            let phase = 'initial';
            new IntersectionObserver(entries => {
                for (const e of entries) console.log('entry:' + e.isIntersecting + ':' +
                    e.boundingClientRect.top + ':' + phase);
            }).observe(target);
            document.addEventListener('scroll', event => {
                event.stopPropagation(); event.stopImmediatePropagation();
                Promise.resolve().then(() => Promise.resolve().then(() => {
                    target.style.top = '800px'; phase = 'microtasks';
                }));
            });
        </script>"#,
    );
    assert_eq!(
        advance(&session, initial.document).1.console,
        ["log: entry:false:1000:initial"]
    );
    session
        .send_input(DocumentInput::Scroll(ScrollInput {
            document: initial.document,
            sequence: 1,
            x: 0.0,
            y: 600.0,
        }))
        .unwrap();
    let (painted, advanced, next, input) = next_report(&session, initial.document);
    assert!(!painted && !advanced, "input only queues the scroll event");
    assert_eq!(next, Some(0));
    assert!(
        input.console.is_empty(),
        "observer ran inside the input task"
    );
    let (painted, scroll) = advance(&session, initial.document);
    assert!(
        painted && scroll.console.is_empty(),
        "scroll update must render the Promise mutation before observer delivery"
    );
    assert_eq!(
        advance(&session, initial.document).1.console,
        ["log: entry:true:200:microtasks"]
    );
    assert!(
        advance(&session, initial.document).1.console.is_empty(),
        "duplicate unchanged entry"
    );
    session.shutdown().unwrap();
}

#[test]
fn native_pointer_hit_testing_keeps_document_coordinates_but_events_use_viewport_coordinates() {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut session = RendererSession::launch(options()).expect("hidden renderer");
    let initial = load_html_document(
        &session,
        121,
        r#"<!doctype html>
        <style>body{margin:0} #target{position:absolute;left:100px;top:1000px;
            width:100px;height:100px;background:red}</style><div id=target></div><script>
            document.addEventListener('mousemove', e => {
                const r = e.target.getBoundingClientRect();
                console.log([e.target.id,e.clientX,e.clientY,e.pageX,e.pageY,
                    e.clientX-r.left,e.clientY-r.top,e.view===window].join(':'));
            });
        </script>"#,
    );
    assert!(advance(&session, initial.document).1.console.is_empty());
    session
        .send_input(DocumentInput::Scroll(ScrollInput {
            document: initial.document,
            sequence: 1,
            x: 0.0,
            y: 600.0,
        }))
        .unwrap();
    let (_, advanced, next, _) = next_report(&session, initial.document);
    assert!(!advanced);
    assert_eq!(next, Some(0));
    assert!(advance(&session, initial.document).1.console.is_empty());
    session
        .send_input(DocumentInput::Pointer(PointerInput {
            document: initial.document,
            sequence: 2,
            phase: PointerPhase::Move,
            button: PointerButton::None,
            buttons: 0,
            x: 125.0,
            y: 1020.0,
            modifiers: InputModifiers::default(),
            target: None,
        }))
        .unwrap();
    assert_eq!(
        next_report(&session, initial.document).3.console,
        ["log: target:125:420:125:1020:25:20:true"]
    );
    session.shutdown().unwrap();
}
