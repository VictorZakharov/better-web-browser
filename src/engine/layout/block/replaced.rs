//! Block-level replaced image sizing and display-list emission.

use super::super::*;

pub(super) struct BlockImage {
    url: String,
    intrinsic_width: f32,
    intrinsic_height: f32,
}

impl<M: TextMeasurer> LayoutEngine<'_, M> {
    pub(super) fn block_image(&self, node: &NodeRef) -> Option<BlockImage> {
        if !matches!(node.tag_name(), Some("img" | "image" | "video")) {
            return None;
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
        })
    }
}

impl BlockImage {
    pub(super) fn outer_width(
        &self,
        node: &NodeRef,
        style: &ComputedStyle,
        horizontal_insets: f32,
    ) -> f32 {
        element_length(
            node,
            "width",
            style.width,
            self.intrinsic_width,
            style.font_size,
        ) + horizontal_insets
    }

    pub(super) fn content_height(
        &self,
        node: &NodeRef,
        style: &ComputedStyle,
        content_width: f32,
    ) -> f32 {
        let scaled_height = if self.intrinsic_width > 0.0
            && (style.width != Length::Auto || node.attr("width").is_some())
        {
            content_width * self.intrinsic_height / self.intrinsic_width
        } else {
            self.intrinsic_height
        };
        element_length(node, "height", style.height, scaled_height, style.font_size)
    }

    pub(super) fn paint(self, node: &NodeRef, output: &mut LayoutOutput, rect: RectF) {
        output.items.push(DisplayItem::Image {
            rect,
            url: self.url,
            alt: node.attr("alt").unwrap_or_default(),
            tint: None,
        });
    }
}
