//! Generated pseudo-element cascade and box-tree materialization.

use super::*;

impl StyleSet {
    pub(crate) fn generated_pseudo(
        &self,
        origin: &NodeRef,
        pseudo: PseudoElement,
    ) -> Option<NodeRef> {
        self.generated_nodes.get(&(origin.id(), pseudo)).cloned()
    }

    pub(crate) fn computed_style_for_pseudo(
        &mut self,
        origin: &NodeRef,
        pseudo: PseudoElement,
    ) -> Option<&ComputedStyle> {
        self.computed_style_for_node(origin)?;
        if !self.pseudo_styles.contains_key(&(origin.id(), pseudo)) {
            let origin_style = self.styles.get(&origin.id())?.clone();
            let style = self.compute_pseudo_style(origin, pseudo, &origin_style).0;
            self.install_pseudo(origin, pseudo, style);
        }
        self.pseudo_styles.get(&(origin.id(), pseudo))
    }

    pub(super) fn sync_generated_pseudos(
        &mut self,
        origin: &NodeRef,
        origin_style: &ComputedStyle,
    ) -> bool {
        let mut touched = false;
        for pseudo in [PseudoElement::Before, PseudoElement::After] {
            touched |= self.pseudo_styles.contains_key(&(origin.id(), pseudo));
            let (style, matched) = self.compute_pseudo_style(origin, pseudo, origin_style);
            if matched {
                touched = true;
                self.install_pseudo(origin, pseudo, style);
            } else {
                self.remove_pseudo(origin.id(), pseudo);
            }
        }
        touched
    }

    pub(super) fn remove_generated_pseudos(&mut self, origins: &HashSet<NodeId>) {
        let keys = self
            .pseudo_styles
            .keys()
            .filter(|(origin, _)| origins.contains(origin))
            .copied()
            .collect::<Vec<_>>();
        for (origin, pseudo) in keys {
            self.remove_pseudo(origin, pseudo);
        }
    }

    fn compute_pseudo_style(
        &self,
        origin: &NodeRef,
        pseudo: PseudoElement,
        origin_style: &ComputedStyle,
    ) -> (ComputedStyle, bool) {
        // Tree-abiding pseudo-elements inherit from their originating element and otherwise use
        // initial values. They do not receive element UA defaults, presentational hints, or the
        // originating element's inline style.
        // https://drafts.csswg.org/css-pseudo/#treelike
        let mut style = ComputedStyle::inherit_from(Some(origin_style));
        style.root_font_size = origin_style.root_font_size;
        let lower_origin = style.clone();
        let matching = self.matching_rules(origin, Some(pseudo));
        let matched = !matching.is_empty();
        self.apply_author_cascade(
            &mut style,
            Some(origin_style),
            &lower_origin,
            &matching,
            &[],
        );
        // Generated pseudo-elements are flex/grid items just like real children. CSS Display
        // blockifies their outer display type at computed-value time, including nonexistent
        // pseudos queried through getComputedStyle.
        if matches!(
            origin_style.display,
            Display::Flex | Display::InlineFlex | Display::Grid
        ) {
            style.display = match style.display {
                Display::Inline => Display::Block,
                Display::InlineBlock => Display::Block,
                Display::InlineFlex => Display::Flex,
                display => display,
            };
        }
        style.resolve_relative_units(
            self.viewport_width,
            self.viewport_height,
            style.root_font_size,
        );
        if matches!(style.position, Position::Absolute | Position::Fixed) {
            style.float = Float::None;
        }
        style.line_height = style.line_height.max(style.font_size);
        (style, matched)
    }

    fn install_pseudo(&mut self, origin: &NodeRef, pseudo: PseudoElement, style: ComputedStyle) {
        let key = (origin.id(), pseudo);
        let generates = style.generated_content.generates_box()
            && style.display != Display::None
            && origin_accepts_generated_children(origin)
            && self
                .styles
                .get(&origin.id())
                .is_none_or(|origin_style| origin_style.display != Display::None);
        self.pseudo_styles.insert(key, style.clone());
        if !generates {
            if let Some(node) = self.generated_nodes.remove(&key) {
                self.remove_generated_node_styles(&node);
            }
            return;
        }

        let contents = style.generated_content.text_for(origin);
        let node = self
            .generated_nodes
            .entry(key)
            .or_insert_with(|| {
                Node::create_generated_pseudo_for(origin, pseudo.tag_name(), &contents)
            })
            .clone();
        Node::replace_generated_pseudo_text(&node, &contents);
        self.generated_styles.insert(node.id(), style.clone());
        if let Some(text) = node.children.borrow().first() {
            self.generated_styles
                .insert(text.id(), ComputedStyle::inherit_from(Some(&style)));
        }
    }

    fn remove_pseudo(&mut self, origin: NodeId, pseudo: PseudoElement) {
        self.pseudo_styles.remove(&(origin, pseudo));
        if let Some(node) = self.generated_nodes.remove(&(origin, pseudo)) {
            self.remove_generated_node_styles(&node);
        }
    }

    fn remove_generated_node_styles(&mut self, node: &NodeRef) {
        self.generated_styles.remove(&node.id());
        for child in node.children.borrow().iter() {
            self.generated_styles.remove(&child.id());
        }
    }
}

impl PseudoElement {
    fn tag_name(self) -> &'static str {
        match self {
            Self::Before => "breeze-pseudo-before",
            Self::After => "breeze-pseudo-after",
        }
    }
}

fn origin_accepts_generated_children(origin: &NodeRef) -> bool {
    !matches!(
        origin.tag_name(),
        Some(
            "area"
                | "base"
                | "br"
                | "canvas"
                | "embed"
                | "hr"
                | "iframe"
                | "img"
                | "input"
                | "link"
                | "meta"
                | "object"
                | "source"
                | "track"
                | "video"
                | "wbr"
        )
    )
}
