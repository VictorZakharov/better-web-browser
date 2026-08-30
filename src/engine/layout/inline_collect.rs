use super::*;

impl<M: TextMeasurer> LayoutEngine<'_, M> {
    pub(super) fn collect_inline(
        &self,
        node: &NodeRef,
        inherited_link: Option<(String, NodeId)>,
        output: &mut Vec<InlineAtom>,
        pending_space: &mut bool,
        honor_block_boundaries: bool,
    ) {
        let style = self.styles.get(node);
        if style.display == Display::None
            || !style.visibility
            || style_collapses_overflow(style, self.viewport)
        {
            return;
        }
        match &node.data {
            NodeData::Text(text) => {
                collect_text_atoms(
                    &text.borrow(),
                    style,
                    inherited_link,
                    node.id(),
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
                    "img" | "image" | "video" => self.collect_image(node, style, link, output),
                    "input" | "textarea" => self.collect_input(node, style, output),
                    "select" => self.collect_select(node, style, output),
                    "button" => self.collect_button(node, style, output),
                    "svg" => self.collect_svg(node, style, output),
                    _ => {
                        if matches!(style.display, Display::InlineBlock | Display::InlineFlex)
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
                            for child in self.box_children(node).iter() {
                                self.collect_inline(
                                    child,
                                    link.clone(),
                                    &mut children,
                                    &mut child_pending_space,
                                    honor_block_boundaries,
                                );
                            }
                            output.push(InlineAtom::InlineBox {
                                children,
                                style: Box::new(style.clone()),
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
    ) {
        let Some(url) = self.page.image_url(node) else {
            return;
        };
        let intrinsic = self.page.images.get(&url);
        let is_video = node.tag_name() == Some("video");
        let placeholder = is_video && url == crate::engine::page::MEDIA_VIDEO_PLACEHOLDER;
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
        let mut width =
            element_length(node, "width", style.width, intrinsic_width, style.font_size);
        let mut height = element_length(
            node,
            "height",
            style.height,
            intrinsic_height,
            style.font_size,
        );
        if style.width != Length::Auto && style.height == Length::Auto && intrinsic_width > 0.0 {
            height = width * intrinsic_height / intrinsic_width;
        } else if style.height != Length::Auto
            && style.width == Length::Auto
            && intrinsic_height > 0.0
        {
            width = height * intrinsic_width / intrinsic_height;
        }
        let margin = style.margin.resolve(self.viewport.width, style.font_size);
        let padding = style.padding.resolve(self.viewport.width, style.font_size);
        let border = style
            .border_width
            .resolve(self.viewport.width, style.font_size);
        output.push(InlineAtom::Image {
            url,
            alt: node.attr("alt").unwrap_or_default(),
            tint: None,
            width: width + margin.horizontal() + padding.horizontal() + border.horizontal(),
            height: height + margin.vertical() + padding.vertical() + border.vertical(),
            inset_x: margin.left + padding.left + border.left,
            inset_y: margin.top + padding.top + border.top,
            image_width: width,
            image_height: height,
        });
    }

    pub(super) fn collect_svg(
        &self,
        node: &NodeRef,
        style: &ComputedStyle,
        output: &mut Vec<InlineAtom>,
    ) {
        let width = element_length(node, "width", style.width, 24.0, style.font_size);
        let height = element_length(node, "height", style.height, 24.0, style.font_size);
        let key = inline_svg_key(node);
        if self.page.images.contains_key(&key) {
            let margin = style.margin.resolve(self.viewport.width, style.font_size);
            let padding = style.padding.resolve(self.viewport.width, style.font_size);
            output.push(InlineAtom::Image {
                url: key,
                alt: node.attr("aria-label").unwrap_or_default(),
                tint: svg_uses_current_color(node).then_some(style.color),
                width: width + margin.horizontal() + padding.horizontal(),
                height: height + margin.vertical() + padding.vertical(),
                inset_x: margin.left + padding.left,
                inset_y: margin.top + padding.top,
                image_width: width,
                image_height: height,
            });
        } else {
            output.push(InlineAtom::Placeholder { width, height });
        }
    }
}
