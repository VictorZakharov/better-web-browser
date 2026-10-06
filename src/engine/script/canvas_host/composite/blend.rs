use super::Operator;

pub(super) fn color(mode: Operator, backdrop: [f64; 3], source: [f64; 3]) -> Option<[f64; 3]> {
    use Operator::*;
    match mode {
        Hue => Some(set_luminosity(
            set_saturation(source, saturation(backdrop)),
            luminosity(backdrop),
        )),
        Saturation => Some(set_luminosity(
            set_saturation(backdrop, saturation(source)),
            luminosity(backdrop),
        )),
        Color => Some(set_luminosity(source, luminosity(backdrop))),
        Luminosity => Some(set_luminosity(backdrop, luminosity(source))),
        Multiply | Screen | Overlay | Darken | Lighten | ColorDodge | ColorBurn | HardLight
        | SoftLight | Difference | Exclusion => Some(std::array::from_fn(|i| {
            channel(mode, backdrop[i], source[i])
        })),
        _ => None,
    }
}

fn channel(mode: Operator, backdrop: f64, source: f64) -> f64 {
    use Operator::*;
    match mode {
        Multiply => backdrop * source,
        Screen => backdrop + source - backdrop * source,
        Overlay => hard_light(source, backdrop),
        Darken => backdrop.min(source),
        Lighten => backdrop.max(source),
        // Backdrop endpoint tests precede source endpoints in the standard.
        ColorDodge if backdrop == 0.0 => 0.0,
        ColorDodge if source == 1.0 => 1.0,
        ColorDodge => (backdrop / (1.0 - source)).min(1.0),
        ColorBurn if backdrop == 1.0 => 1.0,
        ColorBurn if source == 0.0 => 0.0,
        ColorBurn => 1.0 - ((1.0 - backdrop) / source).min(1.0),
        HardLight => hard_light(backdrop, source),
        SoftLight if source <= 0.5 => backdrop - (1.0 - 2.0 * source) * backdrop * (1.0 - backdrop),
        SoftLight => {
            let d = if backdrop <= 0.25 {
                ((16.0 * backdrop - 12.0) * backdrop + 4.0) * backdrop
            } else {
                backdrop.sqrt()
            };
            backdrop + (2.0 * source - 1.0) * (d - backdrop)
        }
        Difference => (backdrop - source).abs(),
        Exclusion => backdrop + source - 2.0 * backdrop * source,
        _ => source,
    }
}

fn hard_light(backdrop: f64, source: f64) -> f64 {
    if source <= 0.5 {
        2.0 * backdrop * source
    } else {
        1.0 - 2.0 * (1.0 - backdrop) * (1.0 - source)
    }
}

fn luminosity(rgb: [f64; 3]) -> f64 {
    0.3 * rgb[0] + 0.59 * rgb[1] + 0.11 * rgb[2]
}
fn saturation(rgb: [f64; 3]) -> f64 {
    rgb[0].max(rgb[1]).max(rgb[2]) - rgb[0].min(rgb[1]).min(rgb[2])
}
fn set_luminosity(mut rgb: [f64; 3], target: f64) -> [f64; 3] {
    let shift = target - luminosity(rgb);
    for value in &mut rgb {
        *value += shift;
    }
    let lum = luminosity(rgb);
    let low = rgb[0].min(rgb[1]).min(rgb[2]);
    let high = rgb[0].max(rgb[1]).max(rgb[2]);
    if low < 0.0 {
        for value in &mut rgb {
            *value = lum + (*value - lum) * lum / (lum - low);
        }
    }
    if high > 1.0 {
        for value in &mut rgb {
            *value = lum + (*value - lum) * (1.0 - lum) / (high - lum);
        }
    }
    rgb
}
fn set_saturation(rgb: [f64; 3], target: f64) -> [f64; 3] {
    let mut indices = [0, 1, 2];
    indices.sort_by(|a, b| rgb[*a].total_cmp(&rgb[*b]));
    let [low, middle, high] = indices;
    let mut result = [0.0; 3];
    let range = rgb[high] - rgb[low];
    if range > 0.0 {
        result[middle] = (rgb[middle] - rgb[low]) * target / range;
        result[high] = target;
    }
    result
}
