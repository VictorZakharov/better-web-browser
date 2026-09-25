use text_atoms::{collect_text_atoms, pending_space_atom};
mod text_atoms;
use super::*;
mod intrinsic;
mod positioned;
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
        let inherited_link =
            std::iter::successors(Node::composed_parent(node), Node::composed_parent)
                .find(|ancestor| ancestor.tag_name() == Some("a"))
                .and_then(|anchor| {
                    anchor
                        .attr("href")
                        .and_then(|href| resolve_url(&self.page.source_url, &href))
                        .map(|url| (url, anchor.id()))
                });
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
                            || style.mask_image.is_some()
                        {
                            if pending_space.is_some() {
                                output.push(pending_space_atom(pending_space, style, link.clone()));
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

    pub(super) fn collect_image(
        &self,
        node: &NodeRef,
        style: &ComputedStyle,
        _link: Option<(String, NodeId)>,
        output: &mut Vec<InlineAtom>,
        containing_block: InlineContainingBlock,
    ) {
        let is_frame = node.tag_name() == Some("iframe");
        let url = if is_frame {
            Some(String::new())
        } else {
            self.page.image_url(node)
        };
        let intrinsic = url.as_ref().and_then(|url| self.page.images.get(url));
        let is_video = node.tag_name() == Some("video");
        let placeholder =
            is_video && url.as_deref() == Some(crate::engine::page::MEDIA_VIDEO_PLACEHOLDER);
        let intrinsic_width = if is_frame || placeholder {
            300.0
        } else {
            intrinsic.map(|image| image.width as f32).unwrap_or(16.0)
        };
        let intrinsic_height = if is_frame || placeholder {
            150.0
        } else {
            intrinsic.map(|image| image.height as f32).unwrap_or(16.0)
        };
        let specified_width = resolve_replaced_length(
            node,
            "width",
            style.width,
            Some(containing_block.width),
            style.font_size,
        );
        let specified_height = resolve_replaced_length(
            node,
            "height",
            style.height,
            containing_block.height,
            style.font_size,
        );
        let margin = style
            .margin
            .resolve(containing_block.width, style.font_size);
        let padding = style
            .padding
            .resolve(containing_block.width, style.font_size);
        let border = style
            .border_width
            .resolve(containing_block.width, style.font_size);
        let horizontal_insets = padding.horizontal() + border.horizontal();
        let vertical_insets = padding.vertical() + border.vertical();
        let mut width = specified_width.unwrap_or(intrinsic_width);
        let mut height = specified_height.unwrap_or(intrinsic_height);
        // Iframes have 300x150 fallback dimensions, not a natural aspect ratio.
        let natural = (!is_frame).then_some((intrinsic_width, intrinsic_height));
        if specified_width.is_some() && specified_height.is_none() {
            height = aspect_ratio::height_from_width(
                style,
                natural,
                width,
                horizontal_insets,
                vertical_insets,
            )
            .unwrap_or(height);
        } else if specified_height.is_some() && specified_width.is_none() {
            width = aspect_ratio::width_from_height(
                style,
                natural,
                height,
                horizontal_insets,
                vertical_insets,
            )
            .unwrap_or(width);
        }
        (width, height) = replaced_constraints::constrain(
            width,
            height,
            specified_width.is_none(),
            specified_height.is_none(),
            style,
            containing_block,
            self.viewport,
        );
        let outer_width = width + margin.horizontal() + padding.horizontal() + border.horizontal();
        let outer_height = height + margin.vertical() + padding.vertical() + border.vertical();
        if let Some(url) = url {
            output.push(InlineAtom::Image {
                url,
                resize_box: ResizeBox::from_content(width, height, padding, border),
                alt: node.attr("alt").unwrap_or_default(),
                tint: None,
                node_id: node_id(node),
                visible: style.visibility,
                width: outer_width,
                height: outer_height,
                inset_x: margin.left + padding.left + border.left,
                inset_y: margin.top + padding.top + border.top,
                image_width: width,
                image_height: height,
                relative_offset: relative_replaced_offset(style, containing_block),
                transform: style.transform.clone(),
                transform_font_size: style.font_size,
                opacity: style.opacity,
            });
        } else {
            // An img without src is still a replaced element. Retaining its box is required for
            // geometry APIs and for IntersectionObserver-driven lazy source assignment.
            output.push(InlineAtom::Placeholder {
                width: outer_width,
                resize_box: ResizeBox::from_content(width, height, padding, border),
                height: outer_height,
                node_id: Some(node_id(node)),
            });
        }
    }

    pub(super) fn collect_svg(
        &self,
        node: &NodeRef,
        style: &ComputedStyle,
        output: &mut Vec<InlineAtom>,
        containing_block: InlineContainingBlock,
    ) {
        let key = inline_svg_key(node);
        let (intrinsic_width, intrinsic_height) = self
            .page
            .images
            .get(&key)
            .map(|image| (image.width as f32, image.height as f32))
            .unwrap_or((300.0, 150.0));
        let (width, height) = resolve_svg_replaced_size(
            node,
            style,
            containing_block,
            intrinsic_width,
            intrinsic_height,
        );
        if self.page.images.contains_key(&key) {
            let margin = style
                .margin
                .resolve(containing_block.width, style.font_size);
            let border = style
                .border_width
                .resolve(containing_block.width, style.font_size);
            let padding = style
                .padding
                .resolve(containing_block.width, style.font_size);
            output.push(InlineAtom::Image {
                url: key,
                resize_box: ResizeBox::from_content(width, height, padding, border),
                alt: node.attr("aria-label").unwrap_or_default(),
                tint: None,
                node_id: node_id(node),
                visible: style.visibility,
                width: width + margin.horizontal() + border.horizontal() + padding.horizontal(),
                height: height + margin.vertical() + border.vertical() + padding.vertical(),
                inset_x: margin.left + border.left + padding.left,
                inset_y: margin.top + border.top + padding.top,
                image_width: width,
                image_height: height,
                relative_offset: relative_replaced_offset(style, containing_block),
                transform: style.transform.clone(),
                transform_font_size: style.font_size,
                opacity: style.opacity,
            });
        } else {
            output.push(InlineAtom::Placeholder {
                width,
                resize_box: ResizeBox::from_content(
                    width,
                    height,
                    style
                        .padding
                        .resolve(containing_block.width, style.font_size),
                    style
                        .border_width
                        .resolve(containing_block.width, style.font_size),
                ),
                height,
                node_id: Some(node_id(node)),
            });
        }
    }
}
