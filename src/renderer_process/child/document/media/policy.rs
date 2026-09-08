//! Breeze's autoplay policy: silent playback or sticky activation in the owning document.
//! HTML leaves "allowed to play" to the UA; simulated DOM events do not grant activation.
//! https://html.spec.whatwg.org/multipage/media.html#allowed-to-play
use crate::renderer_protocol::{DocumentInput, KeyPhase, PointerButton, PointerPhase};

#[derive(Default)]
pub(in crate::renderer_process::child::document) struct MediaActivation(bool);

impl MediaActivation {
    pub(in crate::renderer_process::child::document) fn observe(&mut self, input: &DocumentInput) {
        self.0 |= match input {
            DocumentInput::Keyboard(key) => {
                key.phase == KeyPhase::Down
                    && key.key != "Escape"
                    && !key.modifiers.control
                    && !key.modifiers.alt
                    && !key.modifiers.meta
            }
            DocumentInput::Pointer(pointer) => {
                matches!(pointer.phase, PointerPhase::Down | PointerPhase::Activate)
                    && pointer.button == PointerButton::Primary
            }
            _ => false,
        };
    }

    pub(super) fn allows(&self, volume_millis: u16) -> bool {
        volume_millis == 0 || self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::renderer_protocol::{DocumentId, InputModifiers, KeyboardInput};

    #[test]
    fn audible_play_requires_document_owned_activation() {
        let mut policy = MediaActivation::default();
        assert!(policy.allows(0));
        assert!(!policy.allows(1000));
        let mut input = KeyboardInput {
            document: DocumentId::new(1).unwrap(),
            sequence: 1,
            phase: KeyPhase::Down,
            key: "Escape".into(),
            code: "Escape".into(),
            repeat: false,
            modifiers: InputModifiers::default(),
            target: None,
        };
        policy.observe(&DocumentInput::Keyboard(input.clone()));
        assert!(!policy.allows(1000));
        input.key = "k".into();
        input.code = "KeyK".into();
        policy.observe(&DocumentInput::Keyboard(input));
        assert!(policy.allows(1000));
        assert!(
            !MediaActivation::default().allows(1000),
            "activation leaked into another document"
        );
    }
}
