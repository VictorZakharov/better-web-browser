//! Weak, mutation-maintained candidates avoid full DOM scans on every ancestor toggle.
use super::*;

const MAX_CANDIDATES: usize = 4096;

#[derive(Default)]
pub(in crate::engine::script) struct InlineTransitions {
    nodes: HashMap<NodeId, std::rc::Weak<Node>>,
}

impl InlineTransitions {
    pub(in crate::engine::script) fn new(document: &NodeRef) -> Self {
        let mut index = Self::default();
        index.inserted(document);
        index
    }

    pub(in crate::engine::script) fn style_changed(&mut self, node: &NodeRef) {
        let candidate = node
            .attr("style")
            .is_some_and(|source| source.to_ascii_lowercase().contains("transition"));
        if !candidate {
            self.nodes.remove(&node.id());
            return;
        }
        if self.nodes.len() == MAX_CANDIDATES {
            self.nodes.retain(|_, node| node.strong_count() != 0);
        }
        if self.nodes.len() < MAX_CANDIDATES {
            self.nodes.insert(node.id(), Rc::downgrade(node));
        }
    }

    pub(in crate::engine::script) fn inserted(&mut self, root: &NodeRef) {
        for node in Node::shadow_including_descendants(root).take(100_000) {
            if node.element().is_some() {
                self.style_changed(&node);
            }
        }
    }

    pub(super) fn descendants(&mut self, root: &NodeRef) -> Vec<NodeRef> {
        self.nodes.retain(|_, node| node.strong_count() != 0);
        let mut nodes = self
            .nodes
            .values()
            .filter_map(std::rc::Weak::upgrade)
            .filter(|node| {
                let mut current = Node::composed_parent(node);
                while let Some(ancestor) = current {
                    if ancestor.id() == root.id() {
                        return true;
                    }
                    current = Node::composed_parent(&ancestor);
                }
                false
            })
            .collect::<Vec<_>>();
        // Admission is stable across hash iteration and repeated queries.
        nodes.sort_by_key(|node| node.id());
        nodes.truncate(64);
        nodes
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inline_candidates_follow_style_changes_without_owning_nodes() {
        let document = crate::engine::dom::parse(
            "<main><div style='transition:opacity 1s'></div><p></p></main>",
        );
        let root = document.elements_named("main").next().unwrap();
        let node = document.elements_named("div").next().unwrap();
        let mut index = InlineTransitions::new(&document.document);
        assert_eq!(index.descendants(&root).len(), 1);
        node.set_attr("style", "color:red");
        index.style_changed(&node);
        assert!(index.descendants(&root).is_empty());
        node.set_attr("style", "transition:opacity 1s");
        index.style_changed(&node);
        Node::remove_from_parent(&node);
        assert!(index.descendants(&root).is_empty());
        let id = node.id();
        drop(node);
        index.descendants(&root);
        assert!(!index.nodes.contains_key(&id));
    }

    #[test]
    fn descendant_budget_is_stable_and_does_not_include_unrelated_branches() {
        let source = format!(
            "<main>{}</main><aside><p style='transition:opacity 1s'></p></aside>",
            "<div style='transition:opacity 1s'></div>".repeat(100)
        );
        let document = crate::engine::dom::parse(&source);
        let root = document.elements_named("main").next().unwrap();
        let unrelated = document.elements_named("p").next().unwrap();
        let mut index = InlineTransitions::new(&document.document);
        let first = index.descendants(&root);
        assert_eq!(first.len(), 64);
        let ids = first.iter().map(|node| node.id()).collect::<Vec<_>>();
        for _ in 0..5 {
            assert_eq!(
                index
                    .descendants(&root)
                    .iter()
                    .map(|node| node.id())
                    .collect::<Vec<_>>(),
                ids
            );
        }
        assert!(!ids.contains(&unrelated.id()));
        assert!(ids.windows(2).all(|pair| pair[0] < pair[1]));
    }

    #[test]
    fn dead_candidates_free_index_capacity_for_newly_inserted_targets() {
        let document = crate::engine::dom::parse("<main></main>");
        let root = document.elements_named("main").next().unwrap();
        let mut index = InlineTransitions::new(&document.document);
        let mut retained = Vec::new();
        for _ in 0..MAX_CANDIDATES {
            let node = Node::create_element_for(&root, "div");
            node.set_attr("style", "transition:opacity 1s");
            index.style_changed(&node);
            retained.push(node);
        }
        assert_eq!(index.nodes.len(), MAX_CANDIDATES);
        retained.clear();
        let incoming = Node::create_element_for(&root, "div");
        incoming.set_attr("style", "transition:opacity 1s");
        Node::append_child(&root, incoming.clone());
        index.inserted(&incoming);
        assert_eq!(index.nodes.len(), 1);
        assert_eq!(index.descendants(&root)[0].id(), incoming.id());
    }
}
