//! Block-level replaced image sizing and display-list emission.

use super::super::*;

pub(super) struct BlockImage {
    url: String,
    intrinsic_width: f32,
    intrinsic_height: f32,
    tint: Option<Color>,
    alt: String,
    available: bool,
}

impl<M: TextMeasurer> LayoutEngine<'_, M> {
    pub(super) fn block_image(&self, node: &NodeRef) -> Option<BlockImage> {
        if !matches!(node.tag_name(), Some("img" | "image" | "video" | "svg")) {
            return None;
        }
        if node.tag_name() == Some("svg") {
            let url = inline_svg_key(node);
            let image = self.page.images.get(&url);
            let (intrinsic_width, intrinsic_height) = image
                .map(|image| (image.width as f32, image.height as f32))
                .unwrap_or((300.0, 150.0));
            return Some(BlockImage {
                url,
                intrinsic_width,
                intrinsic_height,
                tint: svg_uses_current_color(node).then_some(self.styles.get(node).color),
                alt: node.attr("aria-label").unwrap_or_default(),
                available: image.is_some(),
            });
        }
        let url = self.page.image_url(node)?;
        let default_size = if node.tag_name() == Some("video") {
            (300.0, 150.0)
        } else {
            (16.0, 16.0)
        };
        let (intrinsic_width, intrinsic_height) =
            if url == crate::engine::page::MEDIA_VIDEO_PLACEHOLDER {
                default_size
            } else {
                self.page
                    .images
                    .get(&url)
                    .map(|image| (image.width as f32, image.height as f32))
                    .unwrap_or(default_size)
            };
        Some(BlockImage {
            url,
            intrinsic_width,
            intrinsic_height,
            tint: None,
            alt: node.attr("alt").unwrap_or_default(),
            available: true,
        })
    }
}

impl BlockImage {
    pub(super) fn outer_width(
        &self,
        node: &NodeRef,
        style: &ComputedStyle,
        percentage_basis: f32,
        horizontal_insets: f32,
    ) -> f32 {
        self.resolve_length(
            node,
            "width",
            style.width,
            Some(percentage_basis),
            style.font_size,
        )
        .unwrap_or(self.intrinsic_width)
            + horizontal_insets
    }

    pub(super) fn content_height(
        &self,
        node: &NodeRef,
        style: &ComputedStyle,
        content_width: f32,
        percentage_basis: Option<f32>,
    ) -> f32 {
        let scaled_height = if self.intrinsic_width > 0.0
            && (style.width != Length::Auto || node.attr("width").is_some())
        {
            content_width * self.intrinsic_height / self.intrinsic_width
        } else {
            self.intrinsic_height
        };
        self.resolve_length(
            node,
            "height",
            style.height,
            percentage_basis,
            style.font_size,
        )
        .unwrap_or(scaled_height)
    }

    fn resolve_length(
        &self,
        node: &NodeRef,
        attribute: &str,
        css: Length,
        percentage_basis: Option<f32>,
        font_size: f32,
    ) -> Option<f32> {
        if node.tag_name() == Some("svg") {
            resolve_svg_replaced_length(node, attribute, css, percentage_basis, font_size)
        } else {
            resolve_replaced_length(node, attribute, css, percentage_basis, font_size)
        }
    }

    pub(super) fn paint(self, _node: &NodeRef, output: &mut LayoutOutput, rect: RectF) {
        if !self.available {
            return;
        }
        output.items.push(DisplayItem::Image {
            rect,
            url: self.url,
            alt: self.alt,
            tint: self.tint,
        });
    }
}
