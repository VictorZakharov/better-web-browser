use super::*;

#[cfg(test)]
mod tests;

pub(crate) fn translate_display_items(items: &mut [DisplayItem], offset_x: f32, offset_y: f32) {
    for item in items {
        let rect = match item {
            DisplayItem::NodeBoundary { .. } | DisplayItem::PaintBoundary { .. } => continue,
            DisplayItem::BeginClip { bounds }
            | DisplayItem::EndClip { bounds }
            | DisplayItem::BeginOpacity { bounds, .. }
            | DisplayItem::BeginMask { bounds, .. }
            | DisplayItem::EndMask { bounds }
            | DisplayItem::EndOpacity { bounds } => bounds,
            DisplayItem::BeginTransform { bounds, matrix } => {
                use crate::engine::affine::Affine2D;
                // Moving an already transformed range also moves its coordinate origin.
                *matrix = Affine2D::translate(f64::from(offset_x), f64::from(offset_y))
                    .then_local(*matrix)
                    .then_local(Affine2D::translate(
                        -f64::from(offset_x),
                        -f64::from(offset_y),
                    ));
                bounds
            }
            DisplayItem::EndTransform { bounds } => bounds,
            DisplayItem::SolidRect { rect, .. }
            | DisplayItem::BorderRect { rect, .. }
            | DisplayItem::Text { rect, .. }
            | DisplayItem::EmbeddedFrame { rect, .. } => rect,
            DisplayItem::Image { rect, clip, .. } => {
                if let Some(clip) = clip {
                    clip.x += offset_x;
                    clip.y += offset_y;
                }
                rect
            }
            DisplayItem::BackgroundImage {
                clip_rect,
                tile_rect,
                ..
            }
            | DisplayItem::CssGradient {
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
            DisplayItem::BeginCssMask { bounds, layers } => {
                translate_display_items(layers, offset_x, offset_y);
                bounds
            }
            DisplayItem::CssBoxShadow { bounds, edge, .. } => {
                bounds.x += offset_x;
                bounds.y += offset_y;
                edge.x += offset_x;
                edge.y += offset_y;
                continue;
            }
        };
        rect.x += offset_x;
        rect.y += offset_y;
    }
}

impl<M: TextMeasurer> LayoutEngine<'_, M> {
    pub(super) fn apply_generated_matrix(
        &mut self,
        matrix: crate::engine::affine::Affine2D,
        item_start: usize,
    ) {
        if let Some((x, y)) = matrix.translation() {
            if x != 0.0 || y != 0.0 {
                translate_display_items(&mut self.output.items[item_start..], x, y);
            }
        } else {
            self.wrap_transform(item_start, matrix);
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
        for descendant in self.styles.descendants(node) {
            self.output
                .transformed
                .boxes
                .translate(descendant.id(), offset_x, offset_y);
            self.output
                .transformed
                .clips
                .translate(descendant.id(), offset_x, offset_y);
            self.output
                .transformed
                .ports
                .translate(descendant.id(), offset_x, offset_y);
            std::sync::Arc::make_mut(&mut self.output.fragments).translate(
                descendant.id(),
                offset_x,
                offset_y,
            );
            if let Some(scroll) = self.output.scroll_boxes.get_mut(&descendant.id()) {
                scroll.port.x += offset_x;
                scroll.port.y += offset_y;
            }
            if let Some(clip) = self.output.clip_paths.get_mut(&descendant.id()) {
                clip.x += offset_x;
                clip.y += offset_y;
            }
            if let Some(rect) = self.output.node_bounds.get_mut(&descendant.id()) {
                rect.x += offset_x;
                rect.y += offset_y;
            }
        }
    }

    pub(super) fn apply_transform_matrix(
        &mut self,
        node_id: NodeId,
        matrix: crate::engine::affine::Affine2D,
        item_start: usize,
    ) {
        let Some(node) = self.styles.node(node_id) else {
            return;
        };
        if matrix == crate::engine::affine::Affine2D::IDENTITY {
            return;
        }
        self.transform_geometry(&node, matrix);
        if let Some((offset_x, offset_y)) = matrix.translation() {
            // Preserve the inexpensive translated display list, but do not move
            // the logical boxes used by offset/client geometry a second time.
            translate_display_items(&mut self.output.items[item_start..], offset_x, offset_y);
        } else {
            self.wrap_transform(item_start, matrix);
        }
    }
}
