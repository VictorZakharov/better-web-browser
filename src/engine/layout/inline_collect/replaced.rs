//! Inline replaced-element collection and intrinsic image sizing.
use super::*;

impl<M: TextMeasurer> LayoutEngine<'_, M> {
    pub(in crate::engine::layout) fn collect_image(
        &self,
        node: &NodeRef,
        style: &ComputedStyle,
        _link: Option<(String, NodeId)>,
        output: &mut Vec<InlineAtom>,
        containing_block: InlineContainingBlock,
    ) {
        let frame = node.content_navigable_size();
        let is_frame = frame.is_some();
        let url = if is_frame {
            Some(String::new())
        } else {
            self.image_url_for(node)
        };
        let intrinsic = url
            .as_ref()
            .filter(|_| !replaced_size_ignores_natural_size(style))
            .and_then(|url| self.decoded_image(url));
        let is_video = node.tag_name() == Some("video");
        let canvas = (node.tag_name() == Some("canvas"))
            .then(|| crate::engine::page::canvas_intrinsic_size(node));
        let placeholder =
            is_video && url.as_deref() == Some(crate::engine::page::MEDIA_VIDEO_PLACEHOLDER);
        let intrinsic_width = style.effective_zoom
            * if let Some((width, _)) = canvas {
                width as f32
            } else if let Some((width, _)) = frame {
                width
            } else if placeholder {
                300.0
            } else {
                intrinsic.map(|image| image.width as f32).unwrap_or(16.0)
            };
        let intrinsic_height = style.effective_zoom
            * if let Some((_, height)) = canvas {
                height as f32
            } else if let Some((_, height)) = frame {
                height
            } else if placeholder {
                150.0
            } else {
                intrinsic.map(|image| image.height as f32).unwrap_or(16.0)
            };
        let specified_width = resolve_replaced_length(
            node,
            "width",
            style.width.clone(),
            Some(containing_block.width),
            style.font_size,
        )
        .map(|v| {
            if style.width == Length::Auto {
                v * style.effective_zoom
            } else {
                v
            }
        });
        let specified_height = resolve_replaced_length(
            node,
            "height",
            style.height.clone(),
            containing_block.height,
            style.font_size,
        )
        .map(|v| {
            if style.height == Length::Auto {
                v * style.effective_zoom
            } else {
                v
            }
        });
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
        let unloaded = intrinsic.is_none() && canvas.is_none() && !placeholder;
        let natural = (!is_frame)
            .then_some((intrinsic_width, intrinsic_height))
            .filter(|_| aspect_ratio::uses_placeholder_ratio(style, unloaded));
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
                transform: style.transform_from_reference(if style.transform_box.uses_content() {
                    RectF {
                        x: 0.0,
                        y: 0.0,
                        width,
                        height,
                    }
                } else {
                    RectF {
                        x: -padding.left - border.left,
                        y: -padding.top - border.top,
                        width: width + padding.horizontal() + border.horizontal(),
                        height: height + padding.vertical() + border.vertical(),
                    }
                }),
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

    pub(in crate::engine::layout) fn collect_svg(
        &self,
        node: &NodeRef,
        style: &ComputedStyle,
        output: &mut Vec<InlineAtom>,
        containing_block: InlineContainingBlock,
    ) {
        let key = inline_svg_key(node);
        let dimensions = self.image_dimensions(&key);
        let (intrinsic_width, intrinsic_height) = dimensions
            .map(|(width, height)| (width as f32, height as f32))
            .unwrap_or((300.0, 150.0));
        let (width, height) = resolve_svg_replaced_size(
            node,
            style,
            containing_block,
            intrinsic_width,
            intrinsic_height,
        );
        if dimensions.is_some() {
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
                transform: style.transform_from_reference(if style.transform_box.uses_content() {
                    RectF {
                        x: 0.0,
                        y: 0.0,
                        width,
                        height,
                    }
                } else {
                    RectF {
                        x: -padding.left - border.left,
                        y: -padding.top - border.top,
                        width: width + padding.horizontal() + border.horizontal(),
                        height: height + padding.vertical() + border.vertical(),
                    }
                }),
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
