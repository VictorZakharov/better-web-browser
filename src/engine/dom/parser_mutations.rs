//! Parser-only mutation journal. Ordinary DOM bindings already queue their own records.
//! Capture ancestry and siblings at mutation time: foster parenting/adoption can move them
//! again before JavaScript resumes. Records live in Dom, not the allocator, avoiding Rc cycles.
use super::super::{Dom, Node, NodeRef};
#[cfg(test)]
#[path = "parser_mutations_tests.rs"]
mod tests;

#[derive(Debug, Clone)]
pub(crate) struct ParserMutation {
    pub target: NodeRef,
    pub ancestors: Vec<NodeRef>,
    pub kind: &'static str,
    pub added: Vec<NodeRef>,
    pub removed: Vec<NodeRef>,
    pub previous: Option<NodeRef>,
    pub next: Option<NodeRef>,
    pub attribute: Option<(String, String)>,
    pub old_value: Option<String>,
}

impl Node {
    pub(crate) fn observe_parser_old_text(&self, enabled: bool) {
        let count = self.identity.parser_old_text_observers.get();
        self.identity.parser_old_text_observers.set(if enabled {
            count.saturating_add(1)
        } else {
            count.saturating_sub(1)
        });
    }

    pub(crate) fn observe_parser_mutations(&self, enabled: bool) {
        let count = self.identity.parser_observers.get();
        self.identity.parser_observers.set(if enabled {
            count.saturating_add(1)
        } else {
            count.saturating_sub(1)
        });
    }
}

impl Dom {
    pub(crate) fn take_parser_mutations(&self) -> Vec<ParserMutation> {
        std::mem::take(&mut *self.parser_mutations.borrow_mut())
    }

    pub(super) fn parser_record(
        &self,
        target: &NodeRef,
        kind: &'static str,
    ) -> Option<ParserMutation> {
        (self.observable_parser && self.identity.parser_observers.get() > 0).then(|| {
            ParserMutation {
                target: target.clone(),
                ancestors: std::iter::successors(Some(target.clone()), |node| node.parent())
                    .collect(),
                kind,
                added: Vec::new(),
                removed: Vec::new(),
                previous: None,
                next: None,
                attribute: None,
                old_value: None,
            }
        })
    }

    pub(super) fn queue_parser_record(&self, record: Option<ParserMutation>) {
        if let Some(record) = record {
            self.parser_mutations.borrow_mut().push(record);
        }
    }

    pub(super) fn parser_append_text(&self, node: &NodeRef, text: &str) -> bool {
        if !matches!(node.data, super::super::NodeData::Text(_)) {
            return false;
        }
        let mut record = self.parser_record(node, "characterData");
        if let Some(record) = &mut record
            && self.identity.parser_old_text_observers.get() > 0
        {
            record.old_value = Some(node.text_content());
        }
        if !super::super::mutation::append_to_existing_text(node, text) {
            return false;
        }
        self.queue_parser_record(record);
        true
    }

    pub(super) fn parser_remove(&self, node: &NodeRef) {
        let Some((parent, index)) = super::super::mutation::parent_and_index(node) else {
            return;
        };
        let mut record = self.parser_record(&parent, "childList");
        if let Some(record) = &mut record {
            let children = parent.children.borrow();
            record.previous = index.checked_sub(1).and_then(|i| children.get(i).cloned());
            record.next = children.get(index + 1).cloned();
            record.removed.push(node.clone());
        }
        super::super::mutation::remove_from_parent(node);
        self.queue_parser_record(record);
    }

    pub(super) fn parser_insert(&self, parent: &NodeRef, child: NodeRef, next: Option<&NodeRef>) {
        self.parser_remove(&child);
        let index = next
            .and_then(|next| {
                parent
                    .children
                    .borrow()
                    .iter()
                    .position(|node| node.id() == next.id())
            })
            .unwrap_or(parent.children.borrow().len());
        let mut record = self.parser_record(parent, "childList");
        if let Some(record) = &mut record {
            record.previous = index
                .checked_sub(1)
                .and_then(|i| parent.children.borrow().get(i).cloned());
            record.next = next.cloned();
            record.added.push(child.clone());
        }
        child.parent.set(Some(std::rc::Rc::downgrade(parent)));
        parent.children.borrow_mut().insert(index, child.clone());
        Node::checkable_subtree_inserted(&child);
        parent.mark_children_mutated();
        self.queue_parser_record(record);
    }
}
