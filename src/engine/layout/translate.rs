use super::*;

#[cfg(test)]
mod tests;

pub(super) fn translate_display_items(items: &mut [DisplayItem], offset_x: f32, offset_y: f32) {
    for item in items {
        let rect = match item {
            DisplayItem::BeginClip { bounds }
            | DisplayItem::EndClip { bounds }
            | DisplayItem::BeginOpacity { bounds, .. }
            | DisplayItem::EndOpacity { bounds } => bounds,
            DisplayItem::SolidRect { rect, .. }
            | DisplayItem::BorderRect { rect, .. }
            | DisplayItem::Text { rect, .. }
            | DisplayItem::Image { rect, .. } => rect,
            DisplayItem::BackgroundImage {
                clip_rect,
                tile_rect,
                ..
            } => {
                clip_rect.x += offset_x;
                clip_rect.y += offset_y;
                tile_rect.x += offset_x;
                tile_rect.y += offset_y;
                continue;
            }
            DisplayItem::Control(spec) => &mut spec.rect,
        };
        rect.x += offset_x;
        rect.y += offset_y;
    }
}

impl<M: TextMeasurer> LayoutEngine<'_, M> {
    pub(super) fn apply_generated_transform(
        &mut self,
        style: &ComputedStyle,
        border_box: RectF,
        item_start: usize,
    ) {
        let (offset_x, offset_y) =
            style
                .transform
                .resolve(border_box.width, border_box.height, style.font_size);
        if offset_x.abs() > f32::EPSILON || offset_y.abs() > f32::EPSILON {
            translate_display_items(&mut self.output.items[item_start..], offset_x, offset_y);
        }
    }

    pub(super) fn translate_layout_subtree(
        &mut self,
        node: Option<&NodeRef>,
        item_start: usize,
        item_end: usize,
        offset_x: f32,
        offset_y: f32,
    ) {
        translate_display_items(
            &mut self.output.items[item_start..item_end],
            offset_x,
            offset_y,
        );
        let Some(node) = node else {
            return;
        };
        // Assigned nodes belong to the composed box subtree, not necessarily this DOM subtree.
        // Translate their CSSOM bounds with the paint range while excluding unassigned/fallback
        // content that does not participate in these boxes.
        for descendant in Node::composed_descendants(node) {
            if let Some(rect) = self.output.node_bounds.get_mut(&descendant.id()) {
                rect.x += offset_x;
                rect.y += offset_y;
            }
        }
    }

    pub(super) fn apply_transform(
        &mut self,
        node_id: NodeId,
        style: &ComputedStyle,
        border_box: RectF,
        item_start: usize,
    ) {
        let (offset_x, offset_y) =
            style
                .transform
                .resolve(border_box.width, border_box.height, style.font_size);
        if offset_x.abs() <= f32::EPSILON && offset_y.abs() <= f32::EPSILON {
            return;
        }
        let Some(node) = self.page.dom.find_node(node_id) else {
            return;
        };
        let item_end = self.output.items.len();
        self.translate_layout_subtree(Some(&node), item_start, item_end, offset_x, offset_y);
    }
}
