use super::*;

#[test]
fn trusted_custom_scheme_anchor_click_reaches_browser_navigation_boundary() {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let session = RendererSession::launch(options()).expect("launch hidden renderer");
    let initial = load_html_document(
        &session,
        109,
        r#"<!doctype html><style>body { margin: 0 } a { display: block; padding: 10px }</style>
        <a href="web+soup:chicken-k%C3%AFwi">Soup link</a>"#,
    );
    session
        .acknowledge_presentation(PresentationAcknowledgement {
            document: initial.document,
            revision: initial.revision,
            presented: true,
            controls_applied: true,
        })
        .unwrap();
    let rect = initial
        .layout
        .items
        .iter()
        .find_map(|item| match item {
            DisplayItem::Text { rect, text, .. } if text == "Soup" => Some(*rect),
            _ => None,
        })
        .expect("custom link text geometry");
    for (sequence, phase) in [(1, PointerPhase::Down), (2, PointerPhase::Up)] {
        session
            .send_input(DocumentInput::Pointer(PointerInput {
                document: initial.document,
                sequence,
                phase,
                button: PointerButton::Primary,
                buttons: if phase == PointerPhase::Down {
                    PointerButton::Primary.mask()
                } else {
                    0
                },
                x: rect.x + rect.width / 2.0,
                y: rect.y + rect.height / 2.0,
                modifiers: InputModifiers::default(),
                target: None,
            }))
            .unwrap();
    }
    let (url, disposition, cause) = wait_for_navigation(&session, initial.document);
    assert_eq!(url, "web+soup:chicken-k%C3%AFwi");
    assert_eq!(disposition, NavigationDisposition::CurrentTab);
    assert_eq!(cause, NavigationCause::UserActivation);
}
