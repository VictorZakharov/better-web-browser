//! Native computed and underlying styles share one cascade; effects are a separate origin.
use super::*;

impl StyleSet {
    pub(super) fn compute_style(
        &self,
        node: &NodeRef,
        parent: Option<&ComputedStyle>,
    ) -> ComputedStyle {
        self.compute_style_with_effects(node, parent, true)
    }

    pub(crate) fn underlying_style_for_node(&mut self, node: &NodeRef) -> ComputedStyle {
        let parent = Node::composed_parent(node)
            .and_then(|parent| self.computed_style_for_node(&parent).cloned());
        self.compute_style_with_effects(node, parent.as_ref(), false)
    }

    fn compute_style_with_effects(
        &self,
        node: &NodeRef,
        parent: Option<&ComputedStyle>,
        effects: bool,
    ) -> ComputedStyle {
        let mut style = ComputedStyle::inherit_from(parent);
        // Anonymous text runs receive their direct decorating box's underline,
        // not an inherited CSS property on descendant elements. display:contents
        // has no decorating box. General ancestor-box propagation is separate.
        // https://www.w3.org/TR/css-text-decor-3/#line-decoration
        if matches!(&node.data, NodeData::Text(_) | NodeData::Cdata(_)) {
            style.text_decoration_underline = parent.is_some_and(|parent| {
                !matches!(parent.display, Display::Contents | Display::None)
                    && parent.text_decoration_underline
            });
        }
        style.root_font_size = root_font_size_for(&self.styles, node);
        user_agent::apply_user_agent_defaults(node, &mut style, parent);
        let lower_origin = style.clone();
        // HTML hints precede author rules, including an explicit width/height:auto.
        apply_presentational_hints(node, &mut style);

        let matching = self.matching_rules(node, None);
        let inline_declarations = node
            .attr("style")
            .map(|inline| parse_declarations(&inline))
            .unwrap_or_default();
        let animation_declarations = node
            .animation_style()
            .filter(|_| effects)
            .map(|style| parse_declarations(&style))
            .unwrap_or_default();

        self.apply_author_cascade(
            &mut style,
            AuthorCascadeInput {
                node,
                parent,
                lower_origin: &lower_origin,
                matching: &matching,
                inline_declarations: &inline_declarations,
                animation_declarations: &animation_declarations,
                element_presentation: true,
                transition_declarations: &node
                    .transition_style()
                    .filter(|_| effects)
                    .map(|text| parse_declarations(&text))
                    .unwrap_or_default(),
            },
        );
        style.resolve_relative_units(
            self.viewport_width,
            self.viewport_height,
            style.root_font_size,
        );
        if node.attr("hidden").is_some() || is_hidden_by_html_rendering(node) {
            style.display = Display::None;
        }
        super::fullscreen::apply_fullscreen_ua_style(
            node,
            &mut style,
            self.viewport_width,
            self.viewport_height,
        );
        // CSS 2 makes `float` compute to `none` for absolutely positioned boxes. Resolve this
        // after the cascade so the result is independent of declaration source order.
        if matches!(style.position, Position::Absolute | Position::Fixed) {
            style.float = Float::None;
        }
        style.blockify_float();
        style.resolve_line_height(self.viewport_width, self.viewport_height);
        style.snap_border_widths(self.resolution_dppx);
        // Preserve inherited-map identity across incremental recalculation. Otherwise an
        // unchanged ancestor's rebuilt variable map makes every descendant compare a large
        // equivalent map again. Equality is exact; changed values never reuse stale storage.
        if let Some(previous) = self.styles.get(&node_id(node))
            && previous.custom_properties == style.custom_properties
        {
            style.custom_properties = Arc::clone(&previous.custom_properties);
        }
        style
    }
}
