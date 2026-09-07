//! Pointer designation in the flat tree, shared by scripted and scriptless pages.
use super::{Node, NodeRef};

pub(crate) struct HoverTransition {
    pub previous: Option<NodeRef>,
    pub next: Option<NodeRef>,
    pub leaving: Vec<NodeRef>,
    pub entering: Vec<NodeRef>,
}

impl Node {
    // Selectors defines designation through flat-tree ancestry, including assigned slots.
    // https://www.w3.org/TR/selectors-4/#the-hover-pseudo
    pub(crate) fn update_hover_path(
        previous: &mut Vec<NodeRef>,
        target: Option<NodeRef>,
    ) -> HoverTransition {
        let mut path = Vec::new();
        let mut next = target;
        while let Some(node) = next {
            next = Node::composed_parent(&node);
            if node.element().is_some() {
                path.push(node);
            }
        }
        let old = std::mem::replace(previous, path.clone());
        let common = old
            .iter()
            .rev()
            .zip(path.iter().rev())
            .take_while(|(old, new)| old.id() == new.id())
            .count();
        let leaving = old[..old.len() - common].to_vec();
        let entering = path[..path.len() - common].to_vec();
        for node in &leaving {
            node.set_hovered(false);
        }
        for node in &entering {
            node.set_hovered(true);
        }
        HoverTransition {
            previous: old.first().cloned(),
            next: path.first().cloned(),
            leaving,
            entering,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::dom::ShadowRootMode;

    #[test]
    fn designation_includes_slots_is_not_cloned_and_ignores_repeated_moves() {
        let host = Node::create_element("div");
        let child = Node::create_element_for(&host, "span");
        Node::append_child(&host, child.clone());
        let root = Node::attach_shadow(&host, ShadowRootMode::Closed, false, false, false).unwrap();
        let slot = Node::create_element_for(&host, "slot");
        Node::append_child(&root, slot.clone());
        let mut path = Vec::new();
        let entered = Node::update_hover_path(&mut path, Some(child.clone()));
        assert_eq!(entered.entering.len(), 3);
        assert!(child.is_hovered() && slot.is_hovered() && host.is_hovered());
        let version = host.document_mutation_version();
        let repeated = Node::update_hover_path(&mut path, Some(child.clone()));
        assert!(repeated.entering.is_empty() && repeated.leaving.is_empty());
        assert_eq!(host.document_mutation_version(), version);
        let clone = Node::clone_for(&host, &child, true);
        assert!(!clone.is_hovered());
        Node::update_hover_path(&mut path, None);
        assert!(!child.is_hovered() && !slot.is_hovered() && !host.is_hovered());
    }
}
