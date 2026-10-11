//! Retained affine paint boundaries and exact local hit geometry.
//! ResizeObserver continues to use untransformed layout boxes.
use super::*;
use crate::engine::affine::Affine2D;
mod geometry;
pub(crate) use geometry::Geometry;

impl ComputedStyle {
    pub(crate) fn transform_from_reference(&self, reference: RectF) -> Affine2D {
        let (x, y) =
            self.transform_origin
                .resolve(reference.width, reference.height, self.font_size);
        let origin = Affine2D::translate(f64::from(reference.x + x), f64::from(reference.y + y));
        let matrix =
            self.combined_transform()
                .matrix(reference.width, reference.height, self.font_size);
        origin.then_local(matrix).then_local(Affine2D::translate(
            -f64::from(reference.x + x),
            -f64::from(reference.y + y),
        ))
    }
}

impl<M: TextMeasurer> LayoutEngine<'_, M> {
    pub(super) fn wrap_transform(&mut self, start: usize, matrix: Affine2D) {
        if !self.emit_paint || start >= self.output.items.len() {
            return;
        }
        let Some(bounds) = opacity::range_bounds(&self.output.items[start..]) else {
            return;
        };
        let bounds = matrix.bounds(bounds);
        self.output
            .items
            .insert(start, DisplayItem::BeginTransform { bounds, matrix });
        self.output.items.push(DisplayItem::EndTransform { bounds });
    }

    pub(super) fn transform_geometry(&mut self, node: &NodeRef, matrix: Affine2D) {
        for descendant in self.styles.descendants(node) {
            let id = descendant.id();
            if let Some(rect) = self.output.node_bounds.get_mut(&id) {
                std::sync::Arc::make_mut(&mut self.output.fragments).retain_layout_rect(id, *rect);
                *rect = self.output.transformed.boxes.apply(id, *rect, matrix);
            }
            if let Some(clip) = self.output.clip_paths.get_mut(&id) {
                *clip = self.output.transformed.clips.apply(id, *clip, matrix);
            }
            if let Some(scroll) = self.output.scroll_boxes.get_mut(&id) {
                // Scrolling extents, clamping and client sizes remain layout-space
                // values; only the hit-test clip gets the visual transformation.
                self.output.transformed.ports.apply(id, scroll.port, matrix);
            }
            std::sync::Arc::make_mut(&mut self.output.fragments).transform(id, matrix);
        }
    }
}

#[cfg(test)]
mod tests;
