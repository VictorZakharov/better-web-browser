use super::*;

impl ComputedStyle {
    pub(in crate::engine::css) fn resolve_relative_units(
        &mut self,
        width: f32,
        height: f32,
        root_font_size: f32,
    ) {
        let resolve = |length: Length| {
            length
                .resolve_root_font_units(root_font_size)
                .resolve_viewport_units(width, height)
        };
        self.background_position_x = resolve(self.background_position_x);
        self.background_position_y = resolve(self.background_position_y);
        if let BackgroundSize::Explicit {
            width: background_width,
            height: background_height,
        } = self.background_size
        {
            self.background_size = BackgroundSize::Explicit {
                width: resolve(background_width),
                height: resolve(background_height),
            };
        }
        self.width = resolve(self.width);
        self.height = resolve(self.height);
        self.min_width = resolve(self.min_width);
        self.min_height = resolve(self.min_height);
        self.max_width = resolve(self.max_width);
        self.max_height = resolve(self.max_height);
        self.margin = self
            .margin
            .resolve_relative_units(width, height, root_font_size);
        self.padding = self
            .padding
            .resolve_relative_units(width, height, root_font_size);
        self.border_width = self
            .border_width
            .resolve_relative_units(width, height, root_font_size);
        self.border_radius = resolve(self.border_radius);
        self.top = resolve(self.top);
        self.right = resolve(self.right);
        self.bottom = resolve(self.bottom);
        self.left = resolve(self.left);
        self.flex_basis = resolve(self.flex_basis);
        self.grid_column_gap = resolve(self.grid_column_gap);
        self.grid_row_gap = resolve(self.grid_row_gap);
        self.transform.resolve_root_font_units(root_font_size);
    }
}
