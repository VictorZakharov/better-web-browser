use super::*;

/// Physical CSS-pixel sizes from layout, excluding margins and transforms. Content origins
/// are relative to the padding edge, as required by the ResizeObserver content rectangle.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct ResizeBox {
    pub content: RectF,
    pub border_width: f32,
    pub border_height: f32,
}

impl ResizeBox {
    pub(in crate::engine::layout) fn from_content(
        width: f32,
        height: f32,
        padding: ResolvedEdges,
        border: ResolvedEdges,
    ) -> Self {
        Self {
            content: RectF {
                x: padding.left,
                y: padding.top,
                width,
                height,
            },
            border_width: width + padding.horizontal() + border.horizontal(),
            border_height: height + padding.vertical() + border.vertical(),
        }
    }

    pub(in crate::engine::layout) fn from_border(
        width: f32,
        height: f32,
        padding: ResolvedEdges,
        border: ResolvedEdges,
    ) -> Self {
        Self {
            content: RectF {
                x: padding.left,
                y: padding.top,
                width: (width - padding.horizontal() - border.horizontal()).max(0.0),
                height: (height - padding.vertical() - border.vertical()).max(0.0),
            },
            border_width: width,
            border_height: height,
        }
    }
}
