use super::*;
use crate::renderer_protocol::{DocumentId, InputModifiers, PointerButton, PointerInput};

#[test]
fn silent_move_completion_retains_identity_and_excludes_discrete_or_relative_input() {
    let document = DocumentId::new(7).unwrap();
    let mut input = PointerInput {
        document,
        sequence: 18,
        phase: PointerPhase::Move,
        button: PointerButton::None,
        buttons: 0,
        x: 20.0,
        y: 30.0,
        modifiers: InputModifiers::default(),
        target: None,
    };
    assert_eq!(
        default_move_completion(&DocumentInput::Pointer(input)),
        Some(PointerCursorResult {
            document,
            sequence: 18,
            cursor: PointerCursor::Default,
        })
    );
    for phase in [
        PointerPhase::Down,
        PointerPhase::Up,
        PointerPhase::Leave,
        PointerPhase::Activate,
        PointerPhase::LockedMove,
    ] {
        input.phase = phase;
        assert!(default_move_completion(&DocumentInput::Pointer(input)).is_none());
    }
}
