use super::super::*;

pub(super) struct BlockDecoration {
    border_index: Option<usize>,
    background_index: Option<usize>,
    background_image_indexes: Vec<usize>,
    shadow_indexes: Vec<(usize, usize)>,
    borders: ResolvedEdges,
}
impl<M: TextMeasurer> LayoutEngine<'_, M> {
    pub(super) fn begin_block_decoration(
        &mut self,
        node: &NodeRef,
        style: &ComputedStyle,
        borders: ResolvedEdges,
        rect: RectF,
    ) -> BlockDecoration {
        if self.emit_paint {
            self.output.items.push(DisplayItem::PaintBoundary {
                kind: super::paint_order::DECORATION,
                entering: true,
                level: 0,
                isolates: false,
                node_id: (!node.is_generated_pseudo()).then_some(node.id()),
            });
        }
        let (x, border_y, border_box_width) = (rect.x, rect.y, rect.width);
        let mut shadow_indexes = self.reserve_shadows(style, rect, false);
        // The canvas already paints a propagated root or body background.
        let own_background =
            self.emit_paint && style.visibility && !self.paints_canvas_background(node);
        let background_index = if own_background && style.background_color.alpha > 0 {
            let index = self.output.items.len();
            self.output.items.push(DisplayItem::SolidRect {
                rect: RectF {
                    x,
                    y: border_y,
                    width: border_box_width,
                    height: 0.0,
                },
                color: style.background_color,
                radius: 0.0,
            });
            Some(index)
        } else {
            None
        };
        let mut background_image_indexes = Vec::new();
        if own_background {
            // First specified image is nearest the user; color is below every layer.
            for image in style.background_image.0.iter().rev() {
                if matches!(image, ImageSource::None) {
                    continue;
                }
                background_image_indexes.push(self.output.items.len());
                // Reserve one slot per layer until the auto block height is known.
                self.output.items.push(DisplayItem::SolidRect {
                    rect,
                    color: Color::TRANSPARENT,
                    radius: 0.0,
                });
            }
        }
        shadow_indexes.extend(self.reserve_shadows(style, rect, true));
        let border_index = if self.emit_paint
            && style.visibility
            && style.resolved_border_colors().iter().any(|c| c.alpha > 0)
            && (borders.vertical() > 0.0 || borders.horizontal() > 0.0)
        {
            let index = self.output.items.len();
            self.output.items.push(DisplayItem::BorderRect {
                rect,
                widths: [borders.top, borders.right, borders.bottom, borders.left],
                colors: style.resolved_border_colors(),
                radius: 0.0,
            });
            Some(index)
        } else {
            None
        };
        if self.emit_paint {
            self.output.items.push(DisplayItem::PaintBoundary {
                kind: super::paint_order::DECORATION,
                entering: false,
                level: 0,
                isolates: false,
                node_id: None,
            });
        }
        BlockDecoration {
            border_index,
            background_index,
            background_image_indexes,
            shadow_indexes,
            borders,
        }
    }
    pub(super) fn finish_block_decoration(
        &mut self,
        style: &ComputedStyle,
        rect: RectF,
        decoration: BlockDecoration,
    ) -> f32 {
        let BlockDecoration {
            border_index,
            background_index,
            background_image_indexes,
            shadow_indexes,
            borders,
        } = decoration;
        let radius = resolve_border_radius(style.border_radius.clone(), rect, style.font_size);
        for (at, index) in shadow_indexes {
            if let Some(item) =
                super::super::background::shadows::paint_item(style, index, rect, radius, borders)
            {
                self.output.items[at] = item;
            }
        }
        if let Some(index) = border_index
            && let DisplayItem::BorderRect {
                rect: target,
                radius: target_radius,
                ..
            } = &mut self.output.items[index]
        {
            *target = rect;
            *target_radius = radius;
        }
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
        for (index, image) in background_image_indexes.into_iter().zip(
            style
                .background_image
                .0
                .iter()
                .enumerate()
                .rev()
                .filter(|(_, image)| !matches!(image, ImageSource::None)),
        ) {
            let (layer, image) = image;
            if let Some(item) = self.background_paint_item(style, image, layer, rect, radius) {
                self.output.items[index] = item;
            }
        }
        radius
    }
}
