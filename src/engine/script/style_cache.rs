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
        let mut styles = match self.computed_styles.take() {
            Some((cached_version, styles)) if cached_version == version => styles,
            Some((_, mut styles)) if !invalidation.rebuild_style_rules => {
                styles.clear_computed_styles();
                styles
            }
            _ => {
                // Script execution currently owns inline style sources. External sheets remain in
                // the page resource layer until those lifetimes are unified. Compute only the
                // requested node's ancestor chain; getComputedStyle must not cascade every node in
                // a large document merely to inspect one feature-test element.
                StyleSet::for_computed_style(&self.document, &self.document_url, &[], 1024.0)
            }
        };
        let value = styles
            .computed_style_for_node(node)
            .and_then(|style| resolved_property_value(style, property));
        self.computed_styles = Some((version, styles));
        value
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::dom;

    #[test]
    fn computed_style_cascades_only_the_requested_ancestor_chain() {
        let markup = format!(
            "<style>.target {{ color: #123456 }}</style><main>{}</main>",
            (0..100)
                .map(|index| format!(
                    "<p class='{}'>{index}</p>",
                    if index == 99 { "target" } else { "" }
                ))
                .collect::<String>()
        );
        let dom = dom::parse(&markup);
        let target = dom
            .elements_named("p")
            .find(|node| node.attr("class").as_deref() == Some("target"))
            .unwrap();
        let mut state = HostState::new(
            dom.document.clone(),
            "https://example.com/",
            "UTF-8",
            Rc::new(module_loader::WebModuleLoader::new()),
        );

        assert_eq!(
            state.computed_style_property(&target, "color").as_deref(),
            Some("rgb(18, 52, 86)")
        );
        let computed = &state.computed_styles.as_ref().unwrap().1.styles;
        assert!(computed.len() < 10, "computed {} styles", computed.len());
    }
}
