use super::*;
mod replaced_constraints;

fn inline_child_containing_block(
    style: &ComputedStyle,
    parent: InlineContainingBlock,
    viewport: RectF,
) -> InlineContainingBlock {
    // CSS 2.2 10.1: inline-block establishes a content-box containing block;
    // an ordinary inline span does not establish a new percentage basis.
    if !matches!(style.display, Display::InlineBlock | Display::InlineFlex) {
        return parent;
    }
    let padding = style.padding.resolve(parent.width, style.font_size);
    let border = style.border_width.resolve(parent.width, style.font_size);
    let border_box = style.box_sizing == BoxSizing::BorderBox;
    InlineContainingBlock {
        width: style
            .width
            .resolve(parent.width, style.font_size)
            .map(|width| {
                (width
                    - if border_box {
                        padding.horizontal() + border.horizontal()
                    } else {
                        0.0
                    })
                .max(0.0)
            })
            .unwrap_or(parent.width),
        height: resolve_content_height(
            style.height,
            parent.height,
            viewport,
            style.font_size,
            padding.vertical() + border.vertical(),
            style.box_sizing,
        ),
    }
}

impl<M: TextMeasurer> LayoutEngine<'_, M> {
    pub(super) fn collect_inline(
        &self,
        node: &NodeRef,
        inherited_link: Option<(String, NodeId)>,
        output: &mut Vec<InlineAtom>,
        pending_space: &mut bool,
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
        if !style.visibility {
            // visibility:hidden suppresses painting but still generates layout boxes. Lazy
            // images depend on that geometry before assigning src from an intersection callback.
            if matches!(node.tag_name(), Some("img" | "image" | "video")) {
                self.collect_image(node, style, inherited_link, output, containing_block);
            }
            return;
        }
        match &node.data {
            NodeData::Text(text) => {
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
                        *pending_space = false;
                    }
                    "img" | "image" | "video" => {
                        self.collect_image(node, style, link, output, containing_block)
                    }
                    "input" | "textarea" => {
                        self.collect_input(node, style, output, containing_block)
                    }
                    "select" => self.collect_select(node, style, output, containing_block),
                    "button" => self.collect_button(node, style, output, containing_block),
                    "svg" => self.collect_svg(node, style, output, containing_block),
                    _ => {
                        if matches!(style.display, Display::InlineBlock | Display::InlineFlex)
                            || (style.display == Display::Inline
                                && self.box_children(node).is_empty())
                            || style.margin.left != Length::Px(0.0)
                            || style.margin.right != Length::Px(0.0)
                            || style.padding != Edges::ZERO
                            || style.border_width != Edges::ZERO
                            || style.background_color.alpha > 0
                            || style.background_image.is_some()
                            || style.mask_image.is_some()
                        {
                            if *pending_space {
                                output.push(text_atom(" ".into(), style, link.clone(), None));
                                *pending_space = false;
                            }
                            let mut children = Vec::new();
                            let mut child_pending_space = false;
                            let child_containing_block = inline_child_containing_block(
                                style,
                                containing_block,
                                self.viewport,
                            );
                            for child in self.box_children(node).iter() {
                                self.collect_inline(
                                    child,
                                    link.clone(),
                                    &mut children,
                                    &mut child_pending_space,
                                    honor_block_boundaries,
                                    child_containing_block,
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
        let url = self.page.image_url(node);
        let intrinsic = url.as_ref().and_then(|url| self.page.images.get(url));
        let is_video = node.tag_name() == Some("video");
        let placeholder =
            is_video && url.as_deref() == Some(crate::engine::page::MEDIA_VIDEO_PLACEHOLDER);
        let intrinsic_width = if placeholder {
            300.0
        } else {
            intrinsic.map(|image| image.width as f32).unwrap_or(16.0)
        };
        let intrinsic_height = if placeholder {
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
        let mut width = specified_width.unwrap_or(intrinsic_width);
        let mut height = specified_height.unwrap_or(intrinsic_height);
        if specified_width.is_some() && specified_height.is_none() && intrinsic_width > 0.0 {
            height = width * intrinsic_height / intrinsic_width;
        } else if specified_height.is_some() && specified_width.is_none() && intrinsic_height > 0.0
        {
            width = height * intrinsic_width / intrinsic_height;
        }
        let margin = style
            .margin
            .resolve(containing_block.width, style.font_size);
        let padding = style
            .padding
            .resolve(containing_block.width, style.font_size);
        let border = style
            .border_width
            .resolve(containing_block.width, style.font_size);
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
                tint: svg_uses_current_color(node).then_some(style.color),
                node_id: node_id(node),
                visible: style.visibility,
                width: width + margin.horizontal() + border.horizontal() + padding.horizontal(),
                height: height + margin.vertical() + border.vertical() + padding.vertical(),
                inset_x: margin.left + border.left + padding.left,
                inset_y: margin.top + border.top + padding.top,
                image_width: width,
                image_height: height,
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
