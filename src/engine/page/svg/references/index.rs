//! Immutable ID lookup shared only within one synchronous SVG refresh batch.
//! No author callbacks run in that batch; a later checkpoint gets a fresh index.
use crate::engine::dom::{Node, NodeId, NodeRef};
use crate::limits::{MAX_DOM_NODES, MAX_SVG_SOURCE_BYTES};
use std::{collections::HashMap, rc::Rc};

pub(in crate::engine::page::svg) struct Index {
    pub(super) ids: HashMap<String, NodeRef>,
    nodes: usize,
    key_bytes: usize,
}

#[derive(Default)]
pub(in crate::engine::page::svg) struct IndexCache {
    trees: HashMap<NodeId, Rc<Index>>,
    nodes: usize,
    key_bytes: usize,
    #[cfg(test)]
    pub(super) builds: usize,
}

impl IndexCache {
    pub(super) fn get(&mut self, root: &NodeRef) -> Result<Rc<Index>, String> {
        let tree = Node::tree_root(root);
        if let Some(index) = self.trees.get(&tree.id()) {
            return Ok(index.clone());
        }
        let mut index = Index {
            ids: HashMap::new(),
            nodes: 0,
            key_bytes: 0,
        };
        for node in Node::descendants(&tree) {
            if index.nodes >= MAX_DOM_NODES {
                return Err("SVG reference tree exceeds node budget".into());
            }
            index.nodes += 1;
            if let Some(id) = node.attr("id")
                && let std::collections::hash_map::Entry::Vacant(entry) = index.ids.entry(id)
            {
                index.key_bytes = index.key_bytes.saturating_add(entry.key().len());
                entry.insert(node);
            }
        }
        #[cfg(test)]
        {
            self.builds += 1;
        }
        let index = Rc::new(index);
        // Cache exhaustion declines reuse, not the rendering operation. Keep
        // total retained metadata bounded even across many independent shadows.
        if self.nodes.saturating_add(index.nodes) <= MAX_DOM_NODES
            && self.key_bytes.saturating_add(index.key_bytes) <= MAX_SVG_SOURCE_BYTES
        {
            self.nodes += index.nodes;
            self.key_bytes += index.key_bytes;
            self.trees.insert(tree.id(), index.clone());
        }
        Ok(index)
    }
}

impl Index {
    pub(super) fn retain_id(&self, id: &str, node: NodeId) -> bool {
        self.ids.get(id).is_none_or(|first| first.id() == node)
    }
}

#[cfg(test)]
mod tests;
