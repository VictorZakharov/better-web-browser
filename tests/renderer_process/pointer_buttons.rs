use super::support::*;
use better_web_browser::engine::DisplayItem;
use better_web_browser::renderer_process::{RendererEvent, RendererSession};
use better_web_browser::renderer_protocol::{
    DocumentInput, InputModifiers, KeyPhase, KeyboardInput, PointerButton, PointerInput,
    PointerPhase,
};
use std::time::Duration;

#[test]
fn held_mouse_button_survives_motion_across_renderer_boundary() {
    let _serial = SERIAL.lock().unwrap_or_else(|error| error.into_inner());
    let session = RendererSession::launch(options()).unwrap();
    let initial = load_html_document(
        &session,
        240,
        r#"<!doctype html><style>html,body{margin:0}#pad{width:300px;height:100px;background:red}</style>
        <div id=pad>drag pad</div><p id=result>ready</p><script>
          const events=[];
          for(const type of ['mousedown','mousemove','mouseup']) {
            document.addEventListener(type,event=>{
              events.push(type+':'+event.buttons);
              result.textContent=events.join('|')+(type==='mouseup'?'|done':'');
            });
          }
        </script>"#,
    );
    for (index, (phase, button, x)) in [
        (PointerPhase::Down, PointerButton::Primary, 20.0),
        (PointerPhase::Move, PointerButton::None, 100.0),
        (PointerPhase::Up, PointerButton::Primary, 100.0),
    ]
    .into_iter()
    .enumerate()
    {
        session
            .send_input(DocumentInput::Pointer(PointerInput {
                document: initial.document,
                sequence: index as u64 + 1,
                phase,
                button,
                buttons: if phase == PointerPhase::Up { 0 } else { 1 },
                x,
                y: 20.0,
                modifiers: InputModifiers::default(),
                target: None,
            }))
            .unwrap();
    }
    loop {
        match session.wait_for_event(Duration::from_secs(3)).unwrap() {
            RendererEvent::Presentation(presentation) => {
                let text = presentation
                    .layout
                    .items
                    .iter()
                    .filter_map(|item| match item {
                        DisplayItem::Text { text, .. } => Some(text.as_str()),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
                    .join(" ");
                if text.contains("|done") {
                    assert!(
                        text.contains("mousedown:1|mousemove:1|mouseup:0|done"),
                        "{text}"
                    );
                    break;
                }
            }
            RendererEvent::RuntimeUpdate(_)
            | RendererEvent::Diagnostic { .. }
            | RendererEvent::PointerCursor(_) => {}
            event => panic!("unexpected pointer result: {event:?}"),
        }
    }
}

fn pointer_trace(actions: &[(PointerPhase, PointerButton, u8)]) -> String {
    let _serial = SERIAL.lock().unwrap_or_else(|error| error.into_inner());
    let session = RendererSession::launch(options()).unwrap();
    let initial = load_html_document(
        &session,
        241,
        include_str!("../fixtures/pointer-buttons.html"),
    );
    for (index, &(phase, button, buttons)) in actions.iter().enumerate() {
        session
            .send_input(DocumentInput::Pointer(PointerInput {
                document: initial.document,
                sequence: index as u64 + 1,
                phase,
                button,
                buttons,
                x: 20.0 + index as f32,
                y: 20.0,
                modifiers: InputModifiers::default(),
                target: None,
            }))
            .unwrap();
    }
    session
        .send_input(DocumentInput::Keyboard(KeyboardInput {
            document: initial.document,
            sequence: actions.len() as u64 + 1,
            phase: KeyPhase::Down,
            key: "End".into(),
            code: "End".into(),
            repeat: false,
            modifiers: InputModifiers::default(),
            target: None,
        }))
        .unwrap();
    loop {
        match session.wait_for_event(Duration::from_secs(3)).unwrap() {
            RendererEvent::Presentation(presentation) => {
                assert!(
                    presentation.runtime.errors.is_empty(),
                    "{:?}",
                    presentation.runtime.errors
                );
                if let Some(trace) = presentation
                    .runtime
                    .console
                    .iter()
                    .find(|line| line.contains("POINTER_TRACE "))
                {
                    return trace.split_once("POINTER_TRACE ").unwrap().1.to_owned();
                }
            }
            RendererEvent::RuntimeUpdate(_)
            | RendererEvent::Diagnostic { .. }
            | RendererEvent::PointerCursor(_) => {}
            event => panic!("unexpected pointer trace event: {event:?}"),
        }
    }
}

#[test]
fn chorded_buttons_keep_held_state_and_use_pointermove_for_button_changes() {
    use PointerButton::{None, Primary, Secondary};
    use PointerPhase::{Down, Move, Up};
    let trace = pointer_trace(&[
        (Down, Primary, 1),
        (Down, Secondary, 3),
        (Move, None, 3),
        (Up, Primary, 2),
        (Move, None, 2),
        (Up, Secondary, 0),
    ]);
    assert_eq!(
        trace,
        concat!(
            "pointerdown:0:1|mousedown:0:1|pointermove:2:3|mousedown:2:3|",
            "pointermove:-1:3|mousemove:0:3|pointermove:0:2|mouseup:0:2|click:0:2|",
            "pointermove:-1:2|mousemove:0:2|pointerup:2:0|mouseup:2:0"
        )
    );
}

#[test]
fn release_outside_content_clears_click_ownership_when_pointer_returns() {
    use PointerButton::{None, Primary};
    use PointerPhase::{Down, Leave, Move, Up};
    let trace = pointer_trace(&[
        (Down, Primary, 1),
        (Leave, None, 1),
        (Move, None, 0),
        (Up, Primary, 0),
    ]);
    assert!(trace.contains("pointermove:-1:0|mousemove:0:0"), "{trace}");
    assert!(
        !trace.contains("click:"),
        "stale press generated a click: {trace}"
    );
}

#[test]
fn middle_button_drag_has_buttons_four_and_releases_before_auxiliary_activation() {
    use PointerButton::{Middle, None};
    use PointerPhase::{Down, Move, Up};
    assert_eq!(
        pointer_trace(&[(Down, Middle, 4), (Move, None, 4), (Up, Middle, 0)]),
        "pointerdown:1:4|mousedown:1:4|pointermove:-1:4|mousemove:0:4|pointerup:1:0|mouseup:1:0"
    );
}
