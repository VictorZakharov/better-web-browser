//! Block effects compose around the completed local paint range, before paint-order sorting.
use super::super::*;

impl<M: TextMeasurer> LayoutEngine<'_, M> {
    pub(super) fn finish_block_effects(
        &mut self,
        node: &NodeRef,
        style: &ComputedStyle,
        rect: RectF,
        insets: (ResolvedEdges, ResolvedEdges),
        start: usize,
    ) {
        self.wrap_mask(style, rect, start);
        self.wrap_clip_path(node, style, rect, start);
        let reference = if style.transform_box.uses_content() {
            rect.inset(insets.0).inset(insets.1)
        } else {
            rect
        };
        let matrix = style.transform_from_reference(reference);
        if node.is_generated_pseudo() {
            self.apply_generated_matrix(matrix, start);
        } else {
            self.apply_transform_matrix(node.id(), matrix, start);
        }
        self.wrap_opacity(start, style.opacity);
        if self.emit_paint && style.position == Position::Sticky {
            self.output.items.insert(
                start,
                DisplayItem::NodeBoundary {
                    node_id: node.id(),
                    entering: true,
                },
            );
            self.output.items.push(DisplayItem::NodeBoundary {
                node_id: node.id(),
                entering: false,
            });
        }
    }
}
