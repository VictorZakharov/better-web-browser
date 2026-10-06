//! Conservative destination support for bilinear sampling of a padded mask.
//! Calculations remain f64 until clipping, so huge finite offsets never wrap.

pub(super) fn translated_support(
    [width, height]: [u32; 2],
    [mask_width, mask_height]: [u32; 2],
    [origin_x, origin_y]: [f64; 2],
    [offset_x, offset_y]: [f64; 2],
) -> [u32; 4] {
    let axis = |limit: u32, extent: u32, origin: f64, offset: f64| {
        // Retain an extra pixel on both sides. Parenthesization in the actual
        // sampler is x-offset-origin; large opposing coordinates may lose a
        // fractional bit differently from origin+offset. Conservative padding
        // avoids pruning a sample near an integer boundary.
        let start = (origin + offset - 2.0).floor().clamp(0.0, f64::from(limit));
        let end = (origin + offset + f64::from(extent) + 1.0)
            .ceil()
            .clamp(0.0, f64::from(limit));
        (start as u32, end as u32)
    };
    let (left, right) = axis(width, mask_width, origin_x, offset_x);
    let (top, bottom) = axis(height, mask_height, origin_y, offset_y);
    [left, top, right, bottom]
}

#[cfg(test)]
mod tests;
