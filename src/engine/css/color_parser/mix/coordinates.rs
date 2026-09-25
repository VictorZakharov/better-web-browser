//! Color-mix coordinate conversions. All conversions retain extended
//! floating-point components until the painter's final sRGB8 boundary.

use super::Space;
use crate::engine::css::color_parser::perceptual::{
    lab_to_xyz_d50, oklab_to_srgb, srgb_to_oklab, xyz_d50_to_lab,
};
use crate::engine::css::color_parser::spaces::{
    A98_TO_XYZ_D65, D50_TO_D65, D65_TO_D50, P3_TO_XYZ_D65, PROPHOTO_TO_XYZ_D50, REC2020_TO_XYZ_D65,
    XYZ_D50_TO_PROPHOTO, XYZ_D65_TO_A98, XYZ_D65_TO_P3, XYZ_D65_TO_REC2020, a98_to_linear,
    linear_to_a98, linear_to_prophoto, linear_to_rec2020, linear_to_srgb, multiply,
    prophoto_to_linear, rec2020_to_linear, srgb_to_linear, srgb_to_xyz_d65, xyz_d65_to_srgb,
};

pub(super) fn from_srgb(space: Space, rgb: [f64; 3]) -> [f64; 3] {
    match space {
        Space::Srgb => rgb,
        Space::SrgbLinear => rgb.map(srgb_to_linear),
        Space::Oklab => srgb_to_oklab(rgb),
        Space::XyzD65 => srgb_to_xyz_d65(rgb),
        Space::XyzD50 => multiply(D65_TO_D50, srgb_to_xyz_d65(rgb)),
        Space::Lab => xyz_d50_to_lab(multiply(D65_TO_D50, srgb_to_xyz_d65(rgb))),
        Space::DisplayP3 => multiply(XYZ_D65_TO_P3, srgb_to_xyz_d65(rgb)).map(linear_to_srgb),
        Space::DisplayP3Linear => multiply(XYZ_D65_TO_P3, srgb_to_xyz_d65(rgb)),
        Space::A98 => multiply(XYZ_D65_TO_A98, srgb_to_xyz_d65(rgb)).map(linear_to_a98),
        Space::ProPhoto => multiply(
            XYZ_D50_TO_PROPHOTO,
            multiply(D65_TO_D50, srgb_to_xyz_d65(rgb)),
        )
        .map(linear_to_prophoto),
        Space::Rec2020 => multiply(XYZ_D65_TO_REC2020, srgb_to_xyz_d65(rgb)).map(linear_to_rec2020),
        Space::Hsl | Space::Hwb | Space::Lch | Space::Oklch => unreachable!("cylindrical mix path"),
    }
}

pub(super) fn to_srgb(space: Space, coordinates: [f64; 3]) -> [f64; 3] {
    match space {
        Space::Srgb => coordinates,
        Space::SrgbLinear => coordinates.map(linear_to_srgb),
        Space::Oklab => oklab_to_srgb(coordinates),
        Space::XyzD65 => xyz_d65_to_srgb(coordinates),
        Space::XyzD50 => xyz_d65_to_srgb(multiply(D50_TO_D65, coordinates)),
        Space::Lab => xyz_d65_to_srgb(multiply(
            D50_TO_D65,
            lab_to_xyz_d50(coordinates[0], coordinates[1], coordinates[2]),
        )),
        Space::DisplayP3 => {
            xyz_d65_to_srgb(multiply(P3_TO_XYZ_D65, coordinates.map(srgb_to_linear)))
        }
        Space::DisplayP3Linear => xyz_d65_to_srgb(multiply(P3_TO_XYZ_D65, coordinates)),
        Space::A98 => xyz_d65_to_srgb(multiply(A98_TO_XYZ_D65, coordinates.map(a98_to_linear))),
        Space::ProPhoto => xyz_d65_to_srgb(multiply(
            D50_TO_D65,
            multiply(PROPHOTO_TO_XYZ_D50, coordinates.map(prophoto_to_linear)),
        )),
        Space::Rec2020 => xyz_d65_to_srgb(multiply(
            REC2020_TO_XYZ_D65,
            coordinates.map(rec2020_to_linear),
        )),
        Space::Hsl | Space::Hwb | Space::Lch | Space::Oklch => unreachable!("cylindrical mix path"),
    }
}
