//! Bilinear filtering interpolates premultiplied color, then unpremultiplies.
//! Sampling clamps to the original image edge, not an artificial crop edge.
pub(super) fn pixel(
    source: &[u8],
    width: u32,
    height: u32,
    x: f64,
    y: f64,
    smooth: bool,
) -> [f64; 4] {
    let offset = |column: f64, row: f64| {
        let column = column.clamp(0.0, f64::from(width - 1)) as usize;
        let row = row.clamp(0.0, f64::from(height - 1)) as usize;
        (row * width as usize + column) * 4
    };
    if !smooth {
        // JS Math.round chooses the larger integer at a half-way point.
        let start = offset((x + 0.5).floor(), (y + 0.5).floor());
        return std::array::from_fn(|channel| f64::from(source[start + channel]));
    }
    let left = x.floor();
    let top = y.floor();
    let fx = x - left;
    let fy = y - top;
    let samples = [
        (left, top, (1.0 - fx) * (1.0 - fy)),
        (left + 1.0, top, fx * (1.0 - fy)),
        (left, top + 1.0, (1.0 - fx) * fy),
        (left + 1.0, top + 1.0, fx * fy),
    ];
    let mut alpha = 0.0;
    let mut channels = [0.0; 3];
    for (column, row, weight) in samples {
        let start = offset(column, row);
        let sample_alpha = f64::from(source[start + 3]) * weight;
        alpha += sample_alpha;
        for channel in 0..3 {
            channels[channel] += f64::from(source[start + channel]) * sample_alpha;
        }
    }
    if alpha == 0.0 {
        [0.0; 4]
    } else {
        [
            channels[0] / alpha,
            channels[1] / alpha,
            channels[2] / alpha,
            alpha,
        ]
    }
}
