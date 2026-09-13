use super::super::*;

pub(super) struct BlockDecoration {
    background_index: Option<usize>,
    background_image_index: Option<usize>,
    mask_image_index: Option<usize>,
}
impl<M: TextMeasurer> LayoutEngine<'_, M> {
    pub(super) fn begin_block_decoration(
        &mut self,
        node: &NodeRef,
        style: &ComputedStyle,
        rect: RectF,
    ) -> BlockDecoration {
        let (x, border_y, border_box_width) = (rect.x, rect.y, rect.width);
        let background_index =
            if self.emit_paint && style.background_color.alpha > 0 && style.mask_image.is_none() {
                let index = self.output.items.len();
                self.output.items.push(DisplayItem::SolidRect {
                    rect: RectF {
                        x,
                        y: border_y,
                        width: border_box_width,
                        height: 0.0,
                    },
                    color: self.effective_background_color(node),
                    radius: 0.0,
                });
                Some(index)
            } else {
                None
            };
        let background_image_index = style
            .background_image
            .as_ref()
            .filter(|_| self.emit_paint)
            .map(|url| {
                let index = self.output.items.len();
                self.output.items.push(DisplayItem::BackgroundImage {
                    clip_rect: RectF {
                        x,
                        y: border_y,
                        width: border_box_width,
                        height: 0.0,
                    },
                    tile_rect: RectF::default(),
                    url: url.clone(),
                    repeat_x: style.background_repeat_x,
                    repeat_y: style.background_repeat_y,
                });
                index
            });
        let mask_image_index = style
            .mask_image
            .as_ref()
            .filter(|_| self.emit_paint)
            .map(|url| {
                let index = self.output.items.len();
                self.output.items.push(DisplayItem::Image {
                    rect: RectF {
                        x,
                        y: border_y,
                        width: border_box_width,
                        height: 0.0,
                    },
                    url: url.clone(),
                    alt: String::new(),
                    tint: Some(style.background_color),
                });
                index
            });
        BlockDecoration {
            background_index,
            background_image_index,
            mask_image_index,
        }
    }
    pub(super) fn finish_block_decoration(
        &mut self,
        style: &ComputedStyle,
        rect: RectF,
        decoration: BlockDecoration,
    ) -> f32 {
        let BlockDecoration {
            background_index,
            background_image_index,
            mask_image_index,
        } = decoration;
        let radius = resolve_border_radius(style.border_radius, rect, style.font_size);
        if let Some(index) = background_index
            && let DisplayItem::SolidRect {
                rect: target,
                radius: target_radius,
                ..
            } = &mut self.output.items[index]
        {
            *target = rect;
            *target_radius = radius;
        }
        if let Some(index) = background_image_index
            && let Some(tile_rect) = self.background_tile_rect(style, rect)
            && let DisplayItem::BackgroundImage {
                clip_rect,
                tile_rect: target_tile,
                ..
            } = &mut self.output.items[index]
        {
            *clip_rect = rect;
            *target_tile = tile_rect;
        }
        if let Some(index) = mask_image_index
            && let DisplayItem::Image { rect: target, .. } = &mut self.output.items[index]
        {
            *target = rect;
        }
        radius
    }
}
