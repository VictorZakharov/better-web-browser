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
}

impl ParserRenderJournal {
    fn record(&self, change: impl FnOnce() -> ParserRenderChange) {
        if self.enabled.get() {
            self.changes.borrow_mut().push(change());
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

    pub(super) fn parser_render_inserted(&self, parent: &NodeRef, child: &NodeRef, appended: bool) {
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
