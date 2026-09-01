//! Incremental `getComputedStyle` cache maintenance inside a retained realm.

use super::*;

impl HostState {
    pub(super) fn offset_parent(&mut self, node: &NodeRef) -> Option<NodeRef> {
        if !self.is_connected(node) || node.element().is_none() || node.tag_name() == Some("body") {
            return None;
        }
        let (version, mut styles) = self.take_offset_parent_styles();
        let parent = (|| {
            let target = styles.computed_style_for_node(node)?.clone();
            if target.display == crate::engine::css::Display::None {
                return None;
            }
            let mut ancestor = node.parent();
            while let Some(candidate) = ancestor {
                ancestor = candidate.parent();
                if candidate.element().is_none() {
                    continue;
                }
                let style = styles.computed_style_for_node(&candidate)?.clone();
                if style.display == crate::engine::css::Display::None {
                    return None;
                }
                let fixed_containing_block = style.establishes_fixed_position_containing_block();
                if target.position == crate::engine::css::Position::Fixed {
                    if fixed_containing_block {
                        return Some(candidate);
                    }
                    continue;
                }
                if style.position != crate::engine::css::Position::Static || fixed_containing_block
                {
                    return Some(candidate);
                }
                if candidate.tag_name() == Some("body")
                    || (target.position == crate::engine::css::Position::Static
                        && matches!(candidate.tag_name(), Some("td" | "th" | "table")))
                {
                    return Some(candidate);
                }
            }
            None
        })();
        self.offset_parent_styles = Some((version, styles));
        parent
    }

    pub(super) fn computed_style_property(
        &mut self,
        node: &NodeRef,
        property: &str,
    ) -> Option<String> {
        self.computed_style_property_for(node, property, None)
    }

    pub(super) fn computed_pseudo_style_property(
        &mut self,
        node: &NodeRef,
        property: &str,
        pseudo: crate::engine::css::PseudoElement,
    ) -> Option<String> {
        self.computed_style_property_for(node, property, Some(pseudo))
    }

    fn computed_style_property_for(
        &mut self,
        node: &NodeRef,
        property: &str,
        pseudo: Option<crate::engine::css::PseudoElement>,
    ) -> Option<String> {
        let (version, mut styles) = self.take_computed_styles();
        let value = match pseudo {
            Some(pseudo) => styles
                .computed_style_for_pseudo(node, pseudo)
                .and_then(|style| resolved_property_value(style, property)),
            None => styles
                .computed_style_for_node(node)
                .and_then(|style| resolved_property_value(style, property)),
        };
        self.computed_styles = Some((version, styles));
        value
    }

    fn take_computed_styles(&mut self) -> (u64, StyleSet) {
        let version = self.document.document_mutation_version();
        let invalidation = self.pending_invalidation.snapshot(self.mutation_count);
        let styles = match self.computed_styles.take() {
            Some((cached_version, styles)) if cached_version == version => styles,
            Some((_, mut styles)) if !invalidation.rebuild_style_rules => {
                styles.clear_computed_styles();
                styles
            }
            _ => {
                // Inline and constructed sheets live in the script-owned DOM. External resource
                // sheets are retained separately by the page layer and are intentionally excluded
                // until those lifetimes are unified.
                StyleSet::for_computed_style_for_media_environment(
                    &self.document,
                    &self.document_url,
                    &[],
                    self.media_environment,
                )
            }
        };
        (version, styles)
    }

    fn take_offset_parent_styles(&mut self) -> (u64, StyleSet) {
        let version = self.document.document_mutation_version();
        let invalidation = self.pending_invalidation.snapshot(self.mutation_count);
        let styles = match self.offset_parent_styles.take() {
            Some((cached_version, styles)) if cached_version == version => styles,
            Some((_, mut styles)) if !invalidation.rebuild_style_rules => {
                styles.clear_computed_styles();
                styles
            }
            _ => {
                let sources = self
                    .stylesheet_sources
                    .iter()
                    .map(|(url, source)| (url.clone(), source.clone()))
                    .collect::<Vec<_>>();
                StyleSet::for_computed_style_for_media_environment(
                    &self.document,
                    &self.document_url,
                    &sources,
                    self.media_environment,
                )
            }
        };
        (version, styles)
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

    #[test]
    fn offset_parent_reuses_parsed_styles_after_dynamic_class_changes() {
        let dom = dom::parse(
            "<main class='positioned'><span id='first'></span><span id='second'></span></main>",
        );
        let main = dom.elements_named("main").next().unwrap();
        let children = dom.elements_named("span").collect::<Vec<_>>();
        let mut state = HostState::new(
            dom.document.clone(),
            "https://example.com/",
            "UTF-8",
            Rc::new(module_loader::WebModuleLoader::new()),
        );
        state.stylesheet_sources.insert(
            "https://example.com/app.css".into(),
            ".positioned { position: relative }".into(),
        );

        assert_eq!(
            state.offset_parent(&children[0]).map(|node| node.id()),
            Some(main.id())
        );
        children[1].set_attr("class", "updated");
        state.record_mutation(Some(&children[1]), MutationKind::Attribute("class"));
        assert_eq!(
            state.offset_parent(&children[1]).map(|node| node.id()),
            Some(main.id())
        );
        assert!(state.offset_parent_styles.is_some());
    }
}
