use super::Length;

impl Length {
    pub fn resolve(self, basis: f32, font_size: f32) -> Option<f32> {
        match self {
            Self::Auto => None,
            Self::Px(value) => Some(value),
            Self::Percent(value) => Some(basis * value / 100.0),
            Self::Em(value) => Some(font_size * value),
            // CSS Values 4 section 6.1.1 uses the initial root size outside an element context.
            // Element computed styles normalize rem against their actual root before layout.
            Self::Rem(value) => Some(16.0 * value),
            Self::Vw(value) | Self::Vh(value) | Self::Vmin(value) | Self::Vmax(value) => {
                Some(basis * value / 100.0)
            }
            Self::Calc {
                px,
                percent,
                em,
                rem,
                vw,
                vh,
                vmin,
                vmax,
            } => Some(
                px + basis * percent / 100.0
                    + font_size * em
                    + 16.0 * rem
                    + basis * vw / 100.0
                    + basis * vh / 100.0
                    + basis * vmin / 100.0
                    + basis * vmax / 100.0,
            ),
        }
    }

    pub(in crate::engine::css) fn resolve_root_font_units(self, root_font_size: f32) -> Self {
        match self {
            Self::Rem(value) => Self::Px(root_font_size * value),
            Self::Calc {
                px,
                percent,
                em,
                rem,
                vw,
                vh,
                vmin,
                vmax,
            } => {
                let px = px + root_font_size * rem;
                if [percent, em, vw, vh, vmin, vmax]
                    .iter()
                    .all(|value| value.abs() <= f32::EPSILON)
                {
                    Self::Px(px)
                } else {
                    Self::Calc {
                        px,
                        percent,
                        em,
                        rem: 0.0,
                        vw,
                        vh,
                        vmin,
                        vmax,
                    }
                }
            }
            value => value,
        }
    }

    pub(in crate::engine::css) fn resolve_viewport_units(self, width: f32, height: f32) -> Self {
        let width = width.max(1.0);
        let height = height.max(1.0);
        let minimum = width.min(height);
        let maximum = width.max(height);
        match self {
            Self::Vw(value) => Self::Px(width * value / 100.0),
            Self::Vh(value) => Self::Px(height * value / 100.0),
            Self::Vmin(value) => Self::Px(minimum * value / 100.0),
            Self::Vmax(value) => Self::Px(maximum * value / 100.0),
            Self::Calc {
                px,
                percent,
                em,
                rem,
                vw,
                vh,
                vmin,
                vmax,
            } => Self::Calc {
                px: px
                    + width * vw / 100.0
                    + height * vh / 100.0
                    + minimum * vmin / 100.0
                    + maximum * vmax / 100.0,
                percent,
                em,
                rem,
                vw: 0.0,
                vh: 0.0,
                vmin: 0.0,
                vmax: 0.0,
            },
            value => value,
        }
    }
}
