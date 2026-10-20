//! Structural parser edits retained for the renderer's next rendering checkpoint.
//! The MutationObserver journal records only observed targets; this one records every
//! parser edit, but only for the navigation parser that opted in. A checkpoint converts
//! the edits into bounded style roots instead of restyling the whole document.
use super::super::{Dom, NodeData, NodeRef};
use std::cell::{Cell, RefCell};

#[cfg(test)]
#[path = "parser_render_changes_tests.rs"]
mod tests;

#[derive(Debug, Clone)]
pub(crate) enum ParserRenderChange {
    /// `child` joined `parent`; `appended` is false when it was inserted before a sibling.
    /// `parent_was_empty` records whether `parent` matched `:empty` before the insertion.
    Inserted {
        parent: NodeRef,
        child: NodeRef,
        appended: bool,
        parent_was_empty: bool,
    },
    Removed {
        parent: NodeRef,
        child: NodeRef,
    },
    /// Character data appended to an existing Text node.
    Text {
        node: NodeRef,
        parent_was_empty: bool,
    },
    /// Attributes merged into an existing element, or a declarative shadow root attached to it.
    Element(NodeRef),
}

#[derive(Debug, Default)]
pub(crate) struct ParserRenderJournal {
    enabled: Cell<bool>,
    changes: RefCell<Vec<ParserRenderChange>>,
    /// Inserted nodes and changed elements, drained separately by script realms.
    registration_roots: RefCell<Vec<NodeRef>>,
    /// More roots arrived than are retained; the next reader walks the whole document.
    registration_overflowed: Cell<bool>,
}

/// Bounds roots that no realm drains, such as those written after parsing finished.
const MAX_REGISTRATION_ROOTS: usize = 65_536;

impl ParserRenderJournal {
    fn record(&self, change: impl FnOnce() -> ParserRenderChange) {
        if self.enabled.get() {
            self.changes.borrow_mut().push(change());
        }
    }

    fn register(&self, root: &NodeRef) {
        if !self.enabled.get() || self.registration_overflowed.get() {
            return;
        }
        let mut roots = self.registration_roots.borrow_mut();
        if roots.len() == MAX_REGISTRATION_ROOTS {
            roots.clear();
            self.registration_overflowed.set(true);
        } else {
            roots.push(root.clone());
        }
    }
}

impl Dom {
    /// Starts retaining parser edits for rendering checkpoints. Only the document parser
    /// that feeds rendering drains the journal; other parsers would accumulate it unread.
    pub(crate) fn retain_parser_render_changes(&self) {
        self.parser_render_changes.enabled.set(true);
    }

    pub(crate) fn take_parser_render_changes(&self) -> Vec<ParserRenderChange> {
        std::mem::take(&mut *self.parser_render_changes.changes.borrow_mut())
    }

    /// The nodes the parser inserted, and the elements whose shadow root or attributes it
    /// changed, since the last call, in that order; `None` when this DOM does not retain parser
    /// edits or dropped some. A script realm registers these subtrees instead of walking the
    /// whole document.
    pub(crate) fn take_parser_registration_roots(&self) -> Option<Vec<NodeRef>> {
        let journal = &self.parser_render_changes;
        let roots = std::mem::take(&mut *journal.registration_roots.borrow_mut());
        (journal.enabled.get() && !journal.registration_overflowed.replace(false)).then_some(roots)
    }

    pub(super) fn parser_render_inserted(&self, parent: &NodeRef, child: &NodeRef, appended: bool) {
        self.parser_render_changes.register(child);
        self.parser_render_changes
            .record(|| ParserRenderChange::Inserted {
                parent: parent.clone(),
                child: child.clone(),
                appended,
                parent_was_empty: matches_empty(parent),
            });
    }

    pub(super) fn parser_render_removed(&self, parent: &NodeRef, child: &NodeRef) {
        self.parser_render_changes
            .record(|| ParserRenderChange::Removed {
                parent: parent.clone(),
                child: child.clone(),
            });
    }

    /// Call before appending, so `:empty` reflects the parent's previous content.
    pub(super) fn parser_render_text(&self, node: &NodeRef) {
        self.parser_render_changes
            .record(|| ParserRenderChange::Text {
                node: node.clone(),
                parent_was_empty: node.parent().is_none_or(|parent| matches_empty(&parent)),
            });
    }

    pub(super) fn parser_render_element(&self, node: &NodeRef) {
        self.parser_render_changes.register(node);
        self.parser_render_changes
            .record(|| ParserRenderChange::Element(node.clone()));
    }
}

/// Mirrors the `:empty` matcher: comments, processing instructions and whitespace-only text
/// do not make an element non-empty.
fn matches_empty(node: &NodeRef) -> bool {
    node.children
        .borrow()
        .iter()
        .all(|child| match &child.data {
            NodeData::Element(_) => false,
            NodeData::Text(text) | NodeData::Cdata(text) => text
                .borrow()
                .chars()
                .all(|character| matches!(character, '\t' | '\n' | '\x0c' | '\r' | ' ')),
            _ => true,
        })
}
