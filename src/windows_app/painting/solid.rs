//! Blend CSS background colors against the actual painted content, including GPU images.
use super::opacity::{OpacityLayer, OpacityLayerStart};
use super::*;
use better_web_browser::engine::RectF;
use better_web_browser::engine::css::Color;

#[allow(clippy::too_many_arguments)]
pub(super) unsafe fn paint(
    dc: Hdc,
    bounds: RectF,
    color: Color,
    radius: f32,
    scroll_y: i32,
    toolbar_height: i32,
    scale: f32,
    content: &Rect,
    dirty: &Rect,
) {
    if color.alpha == 0 {
        return;
    }
    let rect = screen_rect(bounds, scroll_y, toolbar_height, scale);
    if !intersects(&rect, content) {
        return;
    }
    if color.alpha == 255 {
        fill_color_shape(dc, &rect, color.to_colorref(), radius * scale);
        return;
    }
    // Group buffers preserve the existing scene outside the rounded shape. Constant
    // alpha then blends the shape without precompositing it against a guessed backdrop.
    if let OpacityLayerStart::Layer(layer) = OpacityLayer::begin(
        dc,
        bounds,
        f32::from(color.alpha) / 255.0,
        scroll_y,
        toolbar_height,
        scale,
        content,
        dirty,
    ) {
        let source = layer.dc().unwrap_or(dc);
        fill_color_shape(source, &rect, color.to_colorref(), radius * scale);
        layer.finish();
    }
}
