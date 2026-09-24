use super::*;

impl Node {
    pub(crate) fn animation_style(&self) -> Option<String> {
        self.element()?
            .animation_style
            .borrow()
            .as_deref()
            .map(str::to_owned)
    }

    pub(crate) fn set_animation_style(&self, declarations: &str) -> bool {
        let Some(element) = self.element() else {
            return false;
        };
        let next = (!declarations.is_empty()).then(|| Box::<str>::from(declarations));
        let mut slot = element.animation_style.borrow_mut();
        if *slot == next {
            return false;
        }
        *slot = next;
        drop(slot);
        self.mark_mutated();
        true
    }
}
