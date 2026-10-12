//! Horizontal reflection replay from a byte-checked native baked sampling grid.
use super::*;
use crate::windows_app::rendering_resources::Reflection;

#[allow(clippy::too_many_arguments)]
pub(super) unsafe fn paint(
    dc: Hdc,
    source_dc: Hdc,
    bitmap: Hbitmap,
    key: &str,
    image: &DecodedImage,
    destination: Rect,
    bitmaps: &mut ImageBitmaps,
) -> bool {
    let Some(plan) = Reflection::new(dc, destination) else {
        return false;
    };
    let Some(scaled) = bitmaps.reflected_image(key, image, bitmap, plan, dc) else {
        return false;
    };
    let Some(_identity) = super::super::affine::IdentityScope::new(dc) else {
        return false;
    };
    let bounds = plan.bounds();
    crate::windows_app::paint_primitives::paint_alpha_bitmap_from_dc(
        dc,
        source_dc,
        scaled.handle,
        bounds.width() as u32,
        bounds.height() as u32,
        &bounds,
    );
    true
}
