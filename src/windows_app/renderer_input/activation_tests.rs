use super::*;

#[test]
fn mouse_down_authorizes_picker_before_click_without_regranting_on_up() {
    let document = better_web_browser::renderer_protocol::DocumentId::new(1).unwrap();
    let mut down_document = None;
    assert!(pointer_starts_activation(
        PointerPhase::Down,
        PointerButton::Primary,
        document,
        &mut down_document,
    ));
    assert_eq!(down_document, Some(document));
    // The first request consumes the browser ledger; a replay on pointerup gets no new grant.
    assert!(!pointer_starts_activation(
        PointerPhase::Up,
        PointerButton::Primary,
        document,
        &mut down_document,
    ));
    assert!(!pointer_starts_activation(
        PointerPhase::Down,
        PointerButton::None,
        document,
        &mut down_document,
    ));
}
