//! Cache eviction for native effects, which leave selector inputs untouched.
use super::*;

impl StyleSet {
    pub(crate) fn discard_effect_styles(&mut self, target: &NodeRef) {
        // A root font-size effect also changes rem units outside its flat-tree
        // descendants (including detached nodes queried through CSSOM).
        if Node::composed_parent(target)
            .is_some_and(|parent| matches!(parent.data, NodeData::Document))
        {
            self.clear_computed_styles();
            return;
        }
        // Inheritance follows the flat tree. Include the DOM descendants too:
        // unassigned light children and suppressed slot fallback are queryable.
        // https://www.w3.org/TR/css-cascade-5/#inheriting
        let mut affected = HashSet::new();
        let mut pending = vec![target.clone()];
        while let Some(node) = pending.pop() {
            if !affected.insert(node.id()) {
                continue;
            }
            self.styles.remove(&node.id());
            pending.extend(node.children.borrow().iter().cloned());
            pending.extend(Node::composed_children(&node));
            if let Some(shadow) = node.shadow_root() {
                pending.push(shadow);
            }
        }
        self.layout_generations.forget_below(&affected, target);
        self.remove_generated_pseudos(&affected);
    }
}
