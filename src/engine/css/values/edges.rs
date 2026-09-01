use super::{Edges, Length, ResolvedEdges};

impl Edges {
    pub const ZERO: Self = Self {
        top: Length::Px(0.0),
        right: Length::Px(0.0),
        bottom: Length::Px(0.0),
        left: Length::Px(0.0),
    };

    pub fn resolve(self, width: f32, font_size: f32) -> ResolvedEdges {
        ResolvedEdges {
            top: self.top.resolve(width, font_size).unwrap_or(0.0),
            right: self.right.resolve(width, font_size).unwrap_or(0.0),
            bottom: self.bottom.resolve(width, font_size).unwrap_or(0.0),
            left: self.left.resolve(width, font_size).unwrap_or(0.0),
        }
    }

    pub(super) fn resolve_relative_units(
        self,
        width: f32,
        height: f32,
        root_font_size: f32,
    ) -> Self {
        let resolve = |length: Length| {
            length
                .resolve_root_font_units(root_font_size)
                .resolve_viewport_units(width, height)
        };
        Self {
            top: resolve(self.top),
            right: resolve(self.right),
            bottom: resolve(self.bottom),
            left: resolve(self.left),
        }
    }
}
