//! Internal Popover API showing state; the attribute alone does not open a popover.

use super::*;

impl Node {
    pub fn popover_order(&self) -> u64 {
        self.element()
            .map_or(0, |element| element.popover_order.get())
    }

    pub fn is_popover_open(&self) -> bool {
        self.popover_order() != 0
    }

    pub fn set_popover_order(&self, order: u64) {
        let Some(element) = self.element() else {
            return;
        };
        if order != 0 {
            let mut root = self.identity.clone();
            let mut ancestor = self.shadow_including_parent();
            while let Some(node) = ancestor {
                root = node.identity.clone();
                ancestor = node.shadow_including_parent();
            }
            root.popover_opened.set(true);
        }
        if element.popover_order.replace(order) != order {
            self.mark_mutated();
        }
    }

    /// False until a popover of this document, the document node, has opened.
    pub fn has_opened_popover(document: &NodeRef) -> bool {
        document.identity.popover_opened.get()
    }
}
