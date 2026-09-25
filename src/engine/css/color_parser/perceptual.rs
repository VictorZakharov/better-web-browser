//! CSS Color 4 CIE Lab/LCH and Oklab/OkLCH conversion to the sRGB painter.

use super::spaces::{D50_TO_D65, multiply, srgb_to_xyz_d65, xyz_d65_to_srgb};
use super::{ResolvedColor, alpha, components, hue, number_or_percent};

pub(super) fn parse(name: &str, body: &str) -> Option<ResolvedColor> {
    let (tokens, opacity) = components(body, false)?;
    let oklab = name.starts_with("ok");
    let polar = name.ends_with("lch");
    let light_scale = if oklab { 1.0 } else { 100.0 };
    let light = number_or_percent(&tokens[0], light_scale)?.clamp(0.0, light_scale);
    let [a, b] = if polar {
        let chroma_scale = if oklab { 0.4 } else { 150.0 };
        let chroma = number_or_percent(&tokens[1], chroma_scale)?.max(0.0);
        let radians = hue(&tokens[2])?.to_radians();
        [chroma * radians.cos(), chroma * radians.sin()]
    } else {
        let axis_scale = if oklab { 0.4 } else { 125.0 };
        [
            number_or_percent(&tokens[1], axis_scale)?,
            number_or_percent(&tokens[2], axis_scale)?,
        ]
    };
    let xyz = if oklab {
        oklab_to_xyz_d65(light, a, b)
    } else {
        multiply(D50_TO_D65, lab_to_xyz_d50(light, a, b))
    };
    Some(ResolvedColor {
        rgb: xyz_d65_to_srgb(xyz),
        alpha: alpha(opacity.as_deref())?,
    })
}

pub(super) fn lab_to_xyz_d50(light: f64, a: f64, b: f64) -> [f64; 3] {
    // CSS Color 4 §19: CIE Lab's D50 reference white and the piecewise
    // inverse of the CIE f(t) function. Hue/chroma are converted above.
    const EPSILON: f64 = 216.0 / 24389.0;
    const KAPPA: f64 = 24389.0 / 27.0;
    const D50: [f64; 3] = [0.3457 / 0.3585, 1.0, (1.0 - 0.3457 - 0.3585) / 0.3585];
    let fy = (light + 16.0) / 116.0;
    let fx = fy + a / 500.0;
    let fz = fy - b / 200.0;
    let inverse = |value: f64| {
        let cube = value * value * value;
        if cube > EPSILON {
            cube
        } else {
            (116.0 * value - 16.0) / KAPPA
        }
    };
    [
        inverse(fx) * D50[0],
        (if light > KAPPA * EPSILON {
            fy * fy * fy
        } else {
            light / KAPPA
        }) * D50[1],
        inverse(fz) * D50[2],
    ]
}

pub(super) fn xyz_d50_to_lab(xyz: [f64; 3]) -> [f64; 3] {
    const EPSILON: f64 = 216.0 / 24389.0;
    const KAPPA: f64 = 24389.0 / 27.0;
    const D50: [f64; 3] = [0.3457 / 0.3585, 1.0, (1.0 - 0.3457 - 0.3585) / 0.3585];
    let f = [xyz[0] / D50[0], xyz[1] / D50[1], xyz[2] / D50[2]];
    let f = f.map(|value| {
        if value > EPSILON {
            value.cbrt()
        } else {
            (KAPPA * value + 16.0) / 116.0
        }
    });
    [
        116.0 * f[1] - 16.0,
        500.0 * (f[0] - f[1]),
        200.0 * (f[1] - f[2]),
    ]
}

pub(super) fn oklab_to_xyz_d65(light: f64, a: f64, b: f64) -> [f64; 3] {
    // CSS Color 4 §19: undo the LMS cube-root transform, then convert the
    // linear cone responses to D65 XYZ.
    let lms = multiply(
        [
            [1.0, 0.3963377773761749, 0.2158037573099136],
            [1.0, -0.1055613458156586, -0.0638541728258133],
            [1.0, -0.0894841775298119, -1.2914855480194092],
        ],
        [light, a, b],
    )
    .map(|value| value * value * value);
    multiply(
        [
            [1.2268798758459243, -0.5578149944602171, 0.2813910456659647],
            [-0.0405757452148008, 1.112286803280317, -0.0717110580655164],
            [-0.0763729366746601, -0.4214933324022432, 1.5869240198367816],
        ],
        lms,
    )
}

pub(super) fn srgb_to_oklab(rgb: [f64; 3]) -> [f64; 3] {
    let xyz = srgb_to_xyz_d65(rgb);
    let lms = multiply(
        [
            [0.819022437996703, 0.3619062600528904, -0.1288737815209879],
            [0.0329836539323885, 0.9292868615863434, 0.0361446663506424],
            [0.0481771893596242, 0.2642395317527308, 0.6335478284694309],
        ],
        xyz,
    )
    .map(f64::cbrt);
    multiply(
        [
            [0.210454268309314, 0.7936177747023054, -0.0040720430116193],
            [1.9779985324311684, -2.42859224204858, 0.450593709617411],
            [0.0259040424655478, 0.7827717124575296, -0.8086757549230774],
        ],
        lms,
    )
}

pub(super) fn oklab_to_srgb(lab: [f64; 3]) -> [f64; 3] {
    xyz_d65_to_srgb(oklab_to_xyz_d65(lab[0], lab[1], lab[2]))
}
