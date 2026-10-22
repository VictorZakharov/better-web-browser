//! Independent copies of a style set for a second consumer of the same document.
use super::*;

impl StyleSet {
    /// Copies every computed style so a separate owner can refresh it independently. The
    /// script layout snapshot starts from the page's styles this way instead of computing the
    /// whole document again.
    ///
    /// Generated pseudo-element boxes are detached nodes owned by one set, so the copy creates
    /// its own. Layout generations are renumbered: a value recorded against this set must never
    /// match the copy. Caches derived from matching start empty.
    pub(crate) fn fork(&self) -> Self {
        let mut generated_nodes = HashMap::with_capacity(self.generated_nodes.len());
        let mut generated_styles = HashMap::with_capacity(self.generated_styles.len());
        for (&(origin_id, pseudo), node) in &self.generated_nodes {
            let Some(origin) = node.parent() else {
                continue;
            };
            debug_assert_eq!(origin.id(), origin_id);
            let copy =
                Node::create_generated_pseudo_for(&origin, pseudo.tag_name(), &node.text_content());
            let originals = std::iter::once(node.clone()).chain(node.children.borrow().clone());
            let copies = std::iter::once(copy.clone()).chain(copy.children.borrow().clone());
            for (original, copied) in originals.zip(copies) {
                if let Some(style) = self.generated_styles.get(&original.id()) {
                    generated_styles.insert(copied.id(), style.clone());
                }
            }
            generated_nodes.insert((origin_id, pseudo), copy);
        }
        Self {
            styles: self.styles.clone(),
            pseudo_styles: self.pseudo_styles.clone(),
            generated_nodes,
            generated_styles,
            compiled: std::rc::Rc::clone(&self.compiled),
            ancestor_filters: Default::default(),
            revert_layer_variables: Default::default(),
            has_results: self.has_results.clone(),
            defer_nonrendered_descendants: self.defer_nonrendered_descendants,
            deferred_fullscreen_roots: self.deferred_fullscreen_roots.clone(),
            document_base_url: self.document_base_url.clone(),
            viewport_width: self.viewport_width,
            viewport_height: self.viewport_height,
            resolution_dppx: self.resolution_dppx,
            layout_generations: self.layout_generations.renumbered(),
            // Nobody has taken this log from the copy; report every style as possibly new.
            style_log: StyleLog {
                nodes: Vec::new(),
                overflowed: true,
            },
        }
    }
}
