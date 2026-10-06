//! Straight-alpha sRGB compositing shared by owned Canvas layers and paint.
//! https://www.w3.org/TR/compositing-1/#generalformula
//! f64 preserves the existing scalar Canvas rounding. tiny-skia's premultiplied
//! u8 storage is suitable for coverage/shading, but quantizes this contract.

#[path = "composite/blend.rs"]
mod blend;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Operator {
    SourceOver,
    SourceIn,
    SourceOut,
    SourceAtop,
    DestinationOver,
    DestinationIn,
    DestinationOut,
    DestinationAtop,
    Xor,
    Copy,
    Lighter,
    Multiply,
    Screen,
    Overlay,
    Darken,
    Lighten,
    ColorDodge,
    ColorBurn,
    HardLight,
    SoftLight,
    Difference,
    Exclusion,
    Hue,
    Saturation,
    Color,
    Luminosity,
}

impl Operator {
    pub fn transparent_preserves_backdrop(self) -> bool {
        !matches!(
            self,
            Self::SourceIn
                | Self::SourceOut
                | Self::DestinationIn
                | Self::DestinationAtop
                | Self::Copy
        )
    }

    pub fn parse(name: &str) -> Option<Self> {
        Some(match name {
            "source-over" => Self::SourceOver,
            "source-in" => Self::SourceIn,
            "source-out" => Self::SourceOut,
            "source-atop" => Self::SourceAtop,
            "destination-over" => Self::DestinationOver,
            "destination-in" => Self::DestinationIn,
            "destination-out" => Self::DestinationOut,
            "destination-atop" => Self::DestinationAtop,
            "xor" => Self::Xor,
            "copy" => Self::Copy,
            "lighter" => Self::Lighter,
            "multiply" => Self::Multiply,
            "screen" => Self::Screen,
            "overlay" => Self::Overlay,
            "darken" => Self::Darken,
            "lighten" => Self::Lighten,
            "color-dodge" => Self::ColorDodge,
            "color-burn" => Self::ColorBurn,
            "hard-light" => Self::HardLight,
            "soft-light" => Self::SoftLight,
            "difference" => Self::Difference,
            "exclusion" => Self::Exclusion,
            "hue" => Self::Hue,
            "saturation" => Self::Saturation,
            "color" => Self::Color,
            "luminosity" => Self::Luminosity,
            _ => return None,
        })
    }

    fn factors(self, source: f64, backdrop: f64) -> (f64, f64) {
        match self {
            Self::SourceIn => (backdrop, 0.0),
            Self::SourceOut => (1.0 - backdrop, 0.0),
            Self::SourceAtop => (backdrop, 1.0 - source),
            Self::DestinationOver => (1.0 - backdrop, 1.0),
            Self::DestinationIn => (0.0, source),
            Self::DestinationOut => (0.0, 1.0 - source),
            Self::DestinationAtop => (1.0 - backdrop, source),
            Self::Xor => (1.0 - backdrop, 1.0 - source),
            Self::Copy => (1.0, 0.0),
            _ => (1.0, 1.0 - source),
        }
    }

    pub fn pixel(self, destination: &mut [u8], color: [f64; 4], opacity: f64) {
        self.pixel_with_alpha(destination, color, opacity, false);
    }

    pub fn pixel_with_alpha(
        self,
        destination: &mut [u8],
        color: [f64; 4],
        opacity: f64,
        opaque: bool,
    ) {
        if self == Self::SourceOver {
            super::solid_mask::source_over(destination, color, opacity);
            return;
        }
        let sa = color[3] / 255.0 * opacity;
        let da = f64::from(destination[3]) / 255.0;
        let source = [color[0] / 255.0, color[1] / 255.0, color[2] / 255.0];
        let backdrop = [
            f64::from(destination[0]) / 255.0,
            f64::from(destination[1]) / 255.0,
            f64::from(destination[2]) / 255.0,
        ];
        let blended = blend::color(self, backdrop, source);
        let (sf, df) = self.factors(sa, da);
        let composed_alpha = if self == Self::Lighter {
            (sa + da).min(1.0)
        } else {
            sa * sf + da * df
        };
        // Opaque output retains the premultiplied result, not the straight RGB
        // obtained by first rendering to an alpha bitmap and changing alpha later.
        let alpha = if opaque { 1.0 } else { composed_alpha };
        for channel in 0..3 {
            let value = if self == Self::Lighter {
                (sa * source[channel] + da * backdrop[channel]).min(1.0)
            } else if let Some(blended) = &blended {
                sa * (1.0 - da) * source[channel]
                    + da * (1.0 - sa) * backdrop[channel]
                    + sa * da * blended[channel]
            } else {
                sa * sf * source[channel] + da * df * backdrop[channel]
            };
            destination[channel] = if alpha == 0.0 {
                0
            } else {
                (255.0 * value / alpha).round() as u8
            };
        }
        destination[3] = (alpha * 255.0).round() as u8;
    }
}

#[cfg(test)]
#[path = "composite/tests.rs"]
mod tests;

#[cfg(test)]
#[path = "composite/opaque_tests.rs"]
mod opaque_tests;
