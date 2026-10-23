use text_atoms::{collect_text_atoms, pending_space_atom};
mod text_atoms;
use super::*;
mod intrinsic;
mod positioned;
mod replaced;
mod replaced_constraints;
use positioned::relative_replaced_offset;

impl<M: TextMeasurer> LayoutEngine<'_, M> {
    pub(super) fn collect_inline_root(
        &self,
        node: &NodeRef,
        output: &mut Vec<InlineAtom>,
        pending_space: &mut fragments::PendingSpace,
        honor_block_boundaries: bool,
        containing_block: InlineContainingBlock,
    ) {
        // CSS anonymous-block splitting can promote descendants out of a boxless inline anchor.
        // Recover its activation target once at the promoted formatting-root boundary; recursive
        // collection then propagates the link without repeating an ancestor walk for every run.
        let inherited_link = self.recorded_inherited_link(node);
        self.collect_inline(
            node,
            inherited_link,
            output,
            pending_space,
            honor_block_boundaries,
            containing_block,
        );
    }

    pub(super) fn collect_inline(
        &self,
        node: &NodeRef,
        inherited_link: Option<(String, NodeId)>,
        output: &mut Vec<InlineAtom>,
        pending_space: &mut fragments::PendingSpace,
        honor_block_boundaries: bool,
        containing_block: InlineContainingBlock,
    ) {
        let style = self.styles.get(node);
        // Positioned descendants have their own layout pass; they never contribute inline
        // atoms or intrinsic widths to normal flow (CSS 2.2, absolute positioning model).
        if style.display == Display::None
            || matches!(style.position, Position::Absolute | Position::Fixed)
            || style_collapses_overflow(style, self.viewport)
        {
            return;
        }
        match &node.data {
            NodeData::Text(text) | NodeData::Cdata(text) => {
                collect_text_atoms(
                    &text.borrow(),
                    style,
                    inherited_link,
                    (!node.is_generated_pseudo_content()).then_some(node.id()),
                    node.id(),
                    output,
                    pending_space,
                );
            }
            NodeData::Element(_) => {
                // A pending collapsed space keeps its surrounding style when entering
                // nowrap or preserved text; do not lose it or carry it past the span.
                if pending_space.is_some()
                    && (style.white_space == WhiteSpace::NoWrap
                        || style.white_space.preserves_spaces())
                    && let Some(parent) = Node::composed_parent(node)
                    && !self.styles.get(&parent).white_space.preserves_spaces()
                {
                    output.push(pending_space_atom(
                        pending_space,
                        self.styles.get(&parent),
                        Some(parent.id()),
                        inherited_link.clone(),
                    ));
                    *pending_space = None;
                }
                let tag = node.tag_name().unwrap_or_default();
                let link = if tag == "a" {
                    node.attr("href")
                        .and_then(|href| resolve_url(&self.page.source_url, &href))
                        .map(|url| (url, node.id()))
                        .or(inherited_link)
                } else {
                    inherited_link
                };
                match tag {
                    "br" => {
                        output.push(InlineAtom::Break);
                        *pending_space = None;
                    }
                    "img" | "image" | "video" | "iframe" => {
                        self.collect_image(node, style, link, output, containing_block)
                    }
                    "canvas" if self.page.canvas_is_replaced() => {
                        self.collect_image(node, style, link, output, containing_block)
                    }
                    "input"
                        if node
                            .attr("type")
                            .is_some_and(|kind| kind.eq_ignore_ascii_case("image"))
                            && node.attr("src").is_some_and(|src| !src.trim().is_empty()) =>
                    {
                        self.collect_image(node, style, link, output, containing_block)
                    }
                    "input" | "textarea" => {
                        self.collect_input(node, style, output, containing_block)
                    }
                    "select" => self.collect_input(node, style, output, containing_block),
                    "button" => self.collect_button(node, style, output, containing_block),
                    "svg" => self.collect_svg(node, style, output, containing_block),
                    _ => {
                        if matches!(
                            style.display,
                            Display::InlineBlock | Display::InlineFlex | Display::InlineTable
                        ) {
                            if pending_space.is_some() {
                                let parent = Node::composed_parent(node);
                                let surrounding =
                                    parent.as_ref().map(|n| self.styles.get(n)).unwrap_or(style);
                                output.push(pending_space_atom(
                                    pending_space,
                                    surrounding,
                                    parent
                                        .as_ref()
                                        .map_or(node.id(), |parent| parent.id())
                                        .into(),
                                    link.clone(),
                                ));
                                *pending_space = None;
                            }
                            // Atomic inline boxes have an independent formatting context,
                            // including shrink-to-fit sizing, wrapping and block children.
                            output.push(InlineAtom::BlockBox {
                                node: node.clone(),
                                height_basis: containing_block.height,
                            });
                            return;
                        }
                        // CSS 2.2 §10.3.1: auto inline margins are zero, not a reason
                        // to wrap ordinary inline content in an atomic layout box.
                        if (style.display == Display::Inline && self.box_children(node).is_empty())
                            || !matches!(style.margin.left, Length::Auto | Length::Px(0.0))
                            || !matches!(style.margin.right, Length::Auto | Length::Px(0.0))
                            || style.padding != Edges::ZERO
                            || style.border_width != Edges::ZERO
                            || style.background_color.alpha > 0
                            || style.background_image.is_some()
                            || !style.box_shadows.is_empty()
                            || !style.mask_image.0.is_empty()
                        {
                            if pending_space.is_some() {
                                output.push(pending_space_atom(
                                    pending_space,
                                    style,
                                    Some(node.id()),
                                    link.clone(),
                                ));
                                *pending_space = None;
                            }
                            let mut children = Vec::new();
                            let mut child_pending_space = None;
                            for child in self.box_children(node).iter() {
                                self.collect_inline(
                                    child,
                                    link.clone(),
                                    &mut children,
                                    &mut child_pending_space,
                                    honor_block_boundaries,
                                    containing_block,
                                );
                            }
                            output.push(InlineAtom::InlineBox {
                                children,
                                style: Box::new(style.clone()),
                                node_id: (!node.is_generated_pseudo()).then_some(node.id()),
                            });
                        } else if honor_block_boundaries && is_block_level(style.display) {
                            if !output.is_empty()
                                && !matches!(output.last(), Some(InlineAtom::Break))
                            {
                                output.push(InlineAtom::Break);
                            }
                            for child in self.box_children(node).iter() {
                                self.collect_inline(
                                    child,
                                    link.clone(),
                                    output,
                                    pending_space,
                                    honor_block_boundaries,
                                    containing_block,
                                );
                            }
                            if !output.is_empty()
                                && !matches!(output.last(), Some(InlineAtom::Break))
                            {
                                output.push(InlineAtom::Break);
                            }
                        } else {
                            for child in self.box_children(node).iter() {
                                self.collect_inline(
                                    child,
                                    link.clone(),
                                    output,
                                    pending_space,
                                    honor_block_boundaries,
                                    containing_block,
                                );
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }
}
