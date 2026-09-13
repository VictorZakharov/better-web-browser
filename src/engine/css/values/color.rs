#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Color {
    pub red: u8,
    pub green: u8,
    pub blue: u8,
    pub alpha: u8,
}

impl Color {
    pub const BLACK: Self = Self::rgb(0, 0, 0);
    pub const WHITE: Self = Self::rgb(255, 255, 255);
    pub const TRANSPARENT: Self = Self {
        red: 0,
        green: 0,
        blue: 0,
        alpha: 0,
    };

    pub const fn rgb(red: u8, green: u8, blue: u8) -> Self {
        Self {
            red,
            green,
            blue,
            alpha: 255,
        }
    }

    pub fn to_colorref(self) -> u32 {
        self.red as u32 | ((self.green as u32) << 8) | ((self.blue as u32) << 16)
    }

    pub fn composite_over(self, backdrop: Self) -> Self {
        if self.alpha == 255 {
            return self;
        }
        if self.alpha == 0 {
            return backdrop;
        }
        let source_alpha = f32::from(self.alpha) / 255.0;
        let backdrop_alpha = f32::from(backdrop.alpha) / 255.0;
        let output_alpha = source_alpha + backdrop_alpha * (1.0 - source_alpha);
        if output_alpha <= f32::EPSILON {
            return Self::TRANSPARENT;
        }
        let channel = |source: u8, backdrop: u8| {
            ((f32::from(source) * source_alpha
                + f32::from(backdrop) * backdrop_alpha * (1.0 - source_alpha))
                / output_alpha)
                .round()
                .clamp(0.0, 255.0) as u8
        };
        Self {
            red: channel(self.red, backdrop.red),
            green: channel(self.green, backdrop.green),
            blue: channel(self.blue, backdrop.blue),
            alpha: (output_alpha * 255.0).round().clamp(0.0, 255.0) as u8,
        }
    }
}
