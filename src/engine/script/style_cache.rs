//! Incremental `getComputedStyle` cache maintenance inside a retained realm.

use super::*;

impl HostState {
    pub(super) fn computed_style_property(
        &mut self,
        node: &NodeRef,
        property: &str,
    ) -> Option<String> {
        let version = self.document.document_mutation_version();
        let invalidation = self.pending_invalidation.snapshot(self.mutation_count);
        match self.computed_styles.take() {
            Some((cached_version, mut styles))
                if cached_version != version
                    && !invalidation.rebuild_style_rules
                    && !invalidation.is_empty() =>
            {
                let root = invalidation
                    .root
                    .and_then(|wanted| {
                        Node::descendants(&self.document).find(|node| node.id() == wanted)
                    })
                    .unwrap_or_else(|| self.document.clone());
                styles.refresh_subtree(&self.document, &root, &invalidation.removed_nodes);
                self.computed_styles = Some((version, styles));
            }
            Some(cached) if cached.0 == version => self.computed_styles = Some(cached),
            _ => {
                // Script execution currently owns inline style sources. External sheets remain in
                // the page resource layer until those lifetimes are unified.
                let styles =
                    StyleSet::from_document(&self.document, &self.document_url, &[], 1024.0);
                self.computed_styles = Some((version, styles));
            }
        }
        if let Some(value) = self
            .computed_styles
            .as_ref()
            .and_then(|(_, styles)| styles.styles.get(&node.id()))
            .and_then(|style| resolved_property_value(style, property))
        {
            return Some(value);
        }

        let mut root = node.clone();
        while let Some(parent) = root.parent() {
            root = parent;
        }
        let styles = StyleSet::from_document(&root, &self.document_url, &[], 1024.0);
        styles
            .styles
            .get(&node.id())
            .and_then(|style| resolved_property_value(style, property))
    }
}
