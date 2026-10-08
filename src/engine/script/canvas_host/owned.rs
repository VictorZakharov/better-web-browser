//! Closed admission for stateless paints that can consume a bridge-owned copy.
//! Registry/resource operations must retain their ordinary host dispatch and
//! policy checks. These paints retain no author handles and have no DOM effects.
use super::*;

type Painter = fn(&mut [JsValue]) -> JsValue;

pub(in crate::engine::script) fn painter(operation: &str) -> Option<Painter> {
    Some(match operation {
        "canvasResizeImage" => image_resize::resize_owned,
        "canvasFilterColor" => filter_color::paint_owned,
        "canvasFilterGaussian" => filter_gaussian::paint_owned,
        "canvasPaintSolidPath" => solid_path::paint_owned,
        "canvasPaintShadowPath" => shadow_path::paint_owned,
        "canvasPaintSourceLayer" => source_layer::paint_owned,
        "canvasCompositeLayer" => composite_layer::paint_owned,
        "canvasPaintImage" => image_paint::paint_owned,
        "canvasPaintRectangle" => rectangle::paint_owned,
        "canvasPaintGradientMask" => gradient_mask::paint_owned,
        "canvasPaintPatternMask" => pattern_mask::paint_owned,
        "canvasPaintGlyphs" => glyph_paint::paint_owned,
        "canvasPaintSolidMask" => solid_mask::paint_owned,
        "canvasPaintGradientPath" => gradient_mask::paint_path_owned,
        "canvasPaintPatternPath" => pattern_mask::paint_path_owned,
        _ => return None,
    })
}

#[cfg(test)]
mod tests;
