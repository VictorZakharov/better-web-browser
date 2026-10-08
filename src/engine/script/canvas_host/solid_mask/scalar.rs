//! Scalar source-over oracle shared by Canvas painters and prepared coverage.
pub(crate) fn source_over(pixel: &mut [u8], color: [f64; 4], opacity: f64) {
    let source_alpha = color[3] / 255.0 * opacity;
    if source_alpha == 1.0 {
        // Fully covered opaque source-over replaces the backdrop. Keep the
        // same round() conversion as the scalar equation (including fractional
        // channels), without its redundant destination load/divisions.
        for channel in 0..3 {
            pixel[channel] = color[channel].round() as u8;
        }
        pixel[3] = 255;
        return;
    }
    let backdrop_weight = f64::from(pixel[3]) / 255.0 * (1.0 - source_alpha);
    let output_alpha = source_alpha + backdrop_weight;
    if output_alpha == 0.0 {
        pixel.fill(0);
    } else if source_alpha != 0.0 {
        for channel in 0..3 {
            pixel[channel] = ((source_alpha * color[channel]
                + backdrop_weight * f64::from(pixel[channel]))
                / output_alpha)
                .round() as u8;
        }
        pixel[3] = (output_alpha * 255.0).round() as u8;
    }
}
