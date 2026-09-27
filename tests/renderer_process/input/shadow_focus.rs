use super::*;

#[test]
fn keyboard_uses_the_focused_shadow_descendant_over_a_stale_packet_target() {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let session = RendererSession::launch(options()).expect("launch renderer");
    let initial = load_html_document(
        &session,
        133,
        r#"<!doctype html><div id=host>
            <template shadowrootmode=open shadowrootdelegatesfocus>
                <input id=inside value=deep>
            </template>
        </div><input id=outside value=outer><p id=status>ready</p>
        <script>
            const inside = document.querySelector('#host').shadowRoot.querySelector('#inside');
            const outside = document.querySelector('#outside');
            inside.addEventListener('keydown', () => {
                document.querySelector('#status').textContent = 'key:inside';
            });
            outside.addEventListener('keydown', () => {
                document.querySelector('#status').textContent = 'key:outside';
            });
        </script>"#,
    );
    session
        .acknowledge_presentation(PresentationAcknowledgement {
            document: initial.document,
            revision: initial.revision,
            presented: true,
            controls_applied: true,
        })
        .unwrap();
    let control_id = |value: &str| {
        initial.layout.items.iter().find_map(|item| match item {
            DisplayItem::Control(control)
                if control.kind == ControlKind::Text && control.value == value =>
            {
                DocumentNodeId::new(control.node_id.to_wire()).ok()
            }
            _ => None,
        })
    };
    let inside = control_id("deep").expect("shadow control in presentation");
    let outside = control_id("outer").expect("light-DOM control in presentation");
    session
        .send_input(DocumentInput::Focus(FocusInput {
            document: initial.document,
            sequence: 1,
            focused: true,
            target: Some(inside),
        }))
        .unwrap();
    session
        .send_input(DocumentInput::Keyboard(KeyboardInput {
            document: initial.document,
            sequence: 2,
            phase: KeyPhase::Down,
            key: "x".into(),
            code: "KeyX".into(),
            repeat: false,
            modifiers: InputModifiers::default(),
            // The UI may expose a retargeted or stale node, but keyboard input
            // belongs to the browser's current focused area.
            target: Some(outside),
        }))
        .unwrap();
    let updated = wait_for_text(&session, initial.document, "key:");
    assert!(presentation_text(&updated).contains("key:inside"));
}
