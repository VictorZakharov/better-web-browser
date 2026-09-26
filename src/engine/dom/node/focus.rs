//! DOM interaction flags used by dynamic CSS selectors.

use super::*;

impl Node {
    pub fn is_hovered(&self) -> bool {
        self.element().is_some_and(|element| element.hovered.get())
    }

    pub fn set_hovered(&self, hovered: bool) -> bool {
        let Some(element) = self.element() else {
            return false;
        };
        if element.hovered.replace(hovered) == hovered {
            return false;
        }
        self.mark_mutated();
        true
    }

    pub fn is_focused(&self) -> bool {
        self.element().is_some_and(|element| element.focused.get())
    }

    pub fn has_focus_within(&self) -> bool {
        self.element()
            .is_some_and(|element| element.focus_within.get())
    }

    pub fn set_focus_target(previous: Option<&NodeRef>, next: Option<&NodeRef>) {
        if previous.map(|node| node.id()) == next.map(|node| node.id()) {
            return;
        }
        if let Some(previous) = previous {
            if let Some(element) = previous.element()
                && element.focused.replace(false)
            {
                previous.mark_mutated();
            }
            set_focus_within_path(previous, false);
        }
        if let Some(next) = next {
            if let Some(element) = next.element()
                && !element.focused.replace(true)
            {
                next.mark_mutated();
            }
            set_focus_within_path(next, true);
        }
    }

    pub(in crate::engine::dom) fn set_focus_within_ancestors(start: &NodeRef, focused: bool) {
        set_focus_within_path(start, focused);
    }
}

fn set_focus_within_path(node: &NodeRef, focused: bool) {
    let mut ancestor = Some(node.clone());
    while let Some(current) = ancestor {
        if let Some(element) = current.element()
            && element.focus_within.replace(focused) != focused
        {
            current.mark_mutated();
        }
        ancestor = current.shadow_including_parent();
    }
}
