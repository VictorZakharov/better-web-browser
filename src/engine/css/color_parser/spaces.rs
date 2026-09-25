//! CSS Color 4 predefined RGB and XYZ conversions.
//! The destination painter currently stores sRGB8, so conversion happens at
//! computed-value time. The source coordinates are not clamped before the
//! matrix transform; out-of-gamut output is clipped only at that final boundary.

use super::{ResolvedColor, alpha, components, number_or_percent};

pub(super) fn parse(body: &str) -> Option<ResolvedColor> {
    let body = body.trim();
    let gap = body.find(char::is_whitespace)?;
    let space = body[..gap].to_ascii_lowercase();
    let (channels, opacity) = components(body[gap..].trim(), false)?;
    let mut values = [0.0; 3];
    for (index, channel) in channels.iter().enumerate() {
        values[index] = number_or_percent(channel, 1.0)?;
    }
    let rgb = match space.as_str() {
        "srgb" => values,
        "srgb-linear" => values.map(linear_to_srgb),
        "display-p3" => xyz_d65_to_srgb(multiply(P3_TO_XYZ_D65, values.map(srgb_to_linear))),
        "display-p3-linear" => xyz_d65_to_srgb(multiply(P3_TO_XYZ_D65, values)),
        "a98-rgb" => xyz_d65_to_srgb(multiply(A98_TO_XYZ_D65, values.map(a98_to_linear))),
        "prophoto-rgb" => xyz_d65_to_srgb(multiply(
            D50_TO_D65,
            multiply(PROPHOTO_TO_XYZ_D50, values.map(prophoto_to_linear)),
        )),
        "rec2020" => xyz_d65_to_srgb(multiply(REC2020_TO_XYZ_D65, values.map(rec2020_to_linear))),
        "xyz" | "xyz-d65" => xyz_d65_to_srgb(values),
        "xyz-d50" => xyz_d65_to_srgb(multiply(D50_TO_D65, values)),
        _ => return None,
    };
    Some(ResolvedColor {
        rgb,
        alpha: alpha(opacity.as_deref())?,
    })
}

pub(super) fn multiply(matrix: [[f64; 3]; 3], vector: [f64; 3]) -> [f64; 3] {
    matrix.map(|row| row[0] * vector[0] + row[1] * vector[1] + row[2] * vector[2])
}

// CSS Color 4, §19 sample conversion matrices. Values are the standardized
// D65/D50 chromatic adaptation and display-primary transformations.
const XYZ_D65_TO_SRGB: [[f64; 3]; 3] = [
    [12831.0 / 3959.0, -329.0 / 214.0, -1974.0 / 3959.0],
    [
        -851781.0 / 878810.0,
        1648619.0 / 878810.0,
        36519.0 / 878810.0,
    ],
    [705.0 / 12673.0, -2585.0 / 12673.0, 705.0 / 667.0],
];

const SRGB_TO_XYZ_D65: [[f64; 3]; 3] = [
    [506752.0 / 1228815.0, 87881.0 / 245763.0, 12673.0 / 70218.0],
    [87098.0 / 409605.0, 175762.0 / 245763.0, 12673.0 / 175545.0],
    [7918.0 / 409605.0, 87881.0 / 737289.0, 1001167.0 / 1053270.0],
];

pub(super) const P3_TO_XYZ_D65: [[f64; 3]; 3] = [
    [
        608311.0 / 1250200.0,
        189793.0 / 714400.0,
        198249.0 / 1000160.0,
    ],
    [
        35783.0 / 156275.0,
        247089.0 / 357200.0,
        198249.0 / 2500400.0,
    ],
    [0.0, 32229.0 / 714400.0, 5220557.0 / 5000800.0],
];

pub(super) const A98_TO_XYZ_D65: [[f64; 3]; 3] = [
    [
        573536.0 / 994567.0,
        263643.0 / 1420810.0,
        187206.0 / 994567.0,
    ],
    [
        591459.0 / 1989134.0,
        6239551.0 / 9945670.0,
        374412.0 / 4972835.0,
    ],
    [
        53769.0 / 1989134.0,
        351524.0 / 4972835.0,
        4929758.0 / 4972835.0,
    ],
];

pub(super) const PROPHOTO_TO_XYZ_D50: [[f64; 3]; 3] = [
    [0.7977666449006423, 0.13518129740053308, 0.0313477341283922],
    [0.2880748288194013, 0.711835234241873, 0.00008993693872564],
    [0.0, 0.0, 0.8251046025104602],
];

pub(super) const REC2020_TO_XYZ_D65: [[f64; 3]; 3] = [
    [
        63426534.0 / 99577255.0,
        20160776.0 / 139408157.0,
        47086771.0 / 278816314.0,
    ],
    [
        26158966.0 / 99577255.0,
        472592308.0 / 697040785.0,
        8267143.0 / 139408157.0,
    ],
    [0.0, 19567812.0 / 697040785.0, 295819943.0 / 278816314.0],
];

pub(super) const D50_TO_D65: [[f64; 3]; 3] = [
    [0.955473421488075, -0.02309845494876471, 0.06325924320057072],
    [
        -0.0283697093338637,
        1.0099953980813041,
        0.021041441191917323,
    ],
    [
        0.012314014864481998,
        -0.020507649298898964,
        1.330365926242124,
    ],
];

pub(super) const D65_TO_D50: [[f64; 3]; 3] = [
    [
        1.0479297925449969,
        0.022946870601609652,
        -0.05019226628920524,
    ],
    [
        0.02962780877005599,
        0.9904344267538799,
        -0.017073799063418826,
    ],
    [
        -0.009243040646204504,
        0.015055191490298152,
        0.7518742814281371,
    ],
];

pub(super) const XYZ_D65_TO_P3: [[f64; 3]; 3] = [
    [
        446124.0 / 178915.0,
        -333277.0 / 357830.0,
        -72051.0 / 178915.0,
    ],
    [-14852.0 / 17905.0, 63121.0 / 35810.0, 423.0 / 17905.0],
    [11844.0 / 330415.0, -50337.0 / 660830.0, 316169.0 / 330415.0],
];

pub(super) const XYZ_D65_TO_A98: [[f64; 3]; 3] = [
    [
        1829569.0 / 896150.0,
        -506331.0 / 896150.0,
        -308931.0 / 896150.0,
    ],
    [
        -851781.0 / 878810.0,
        1648619.0 / 878810.0,
        36519.0 / 878810.0,
    ],
    [
        16779.0 / 1248040.0,
        -147721.0 / 1248040.0,
        1266979.0 / 1248040.0,
    ],
];

pub(super) const XYZ_D50_TO_PROPHOTO: [[f64; 3]; 3] = [
    [
        1.3457868816471583,
        -0.25557208737979464,
        -0.05110186497554526,
    ],
    [-0.5446307051249019, 1.5082477428451468, 0.02052744743642139],
    [0.0, 0.0, 1.2119675456389452],
];

pub(super) const XYZ_D65_TO_REC2020: [[f64; 3]; 3] = [
    [
        30757411.0 / 17917100.0,
        -6372589.0 / 17917100.0,
        -4539589.0 / 17917100.0,
    ],
    [
        -19765991.0 / 29648200.0,
        47925759.0 / 29648200.0,
        467509.0 / 29648200.0,
    ],
    [
        792561.0 / 44930125.0,
        -1921689.0 / 44930125.0,
        42328811.0 / 44930125.0,
    ],
];

pub(super) fn xyz_d65_to_srgb(xyz: [f64; 3]) -> [f64; 3] {
    multiply(XYZ_D65_TO_SRGB, xyz).map(linear_to_srgb)
}

pub(super) fn srgb_to_xyz_d65(rgb: [f64; 3]) -> [f64; 3] {
    multiply(SRGB_TO_XYZ_D65, rgb.map(srgb_to_linear))
}

pub(super) fn linear_to_srgb(value: f64) -> f64 {
    let absolute = value.abs();
    if absolute <= 0.0031308 {
        value * 12.92
    } else {
        value.signum() * (1.055 * absolute.powf(1.0 / 2.4) - 0.055)
    }
}

pub(super) fn srgb_to_linear(value: f64) -> f64 {
    let absolute = value.abs();
    if absolute <= 0.04045 {
        value / 12.92
    } else {
        value.signum() * ((absolute + 0.055) / 1.055).powf(2.4)
    }
}

// CSS Color 4 §19 uses sign-preserving extended transfer functions, so
// out-of-gamut coordinates must not be clamped before matrix conversion.
pub(super) fn a98_to_linear(value: f64) -> f64 {
    value.signum() * value.abs().powf(563.0 / 256.0)
}

pub(super) fn prophoto_to_linear(value: f64) -> f64 {
    if value.abs() <= 16.0 / 512.0 {
        value / 16.0
    } else {
        value.signum() * value.abs().powf(1.8)
    }
}

pub(super) fn rec2020_to_linear(value: f64) -> f64 {
    value.signum() * value.abs().powf(2.4)
}

pub(super) fn linear_to_a98(value: f64) -> f64 {
    value.signum() * value.abs().powf(256.0 / 563.0)
}

pub(super) fn linear_to_prophoto(value: f64) -> f64 {
    if value.abs() < 1.0 / 512.0 {
        16.0 * value
    } else {
        value.signum() * value.abs().powf(1.0 / 1.8)
    }
}

pub(super) fn linear_to_rec2020(value: f64) -> f64 {
    value.signum() * value.abs().powf(1.0 / 2.4)
}
