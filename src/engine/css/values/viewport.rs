use super::*;

impl ComputedStyle {
    pub(in crate::engine::css) fn resolve_relative_units(
        &mut self,
        width: f32,
        height: f32,
        root_font_size: f32,
    ) {
        self.resolve_scalars(width, height, root_font_size);
        self.resolve_spacing(width, height, root_font_size);
        self.resolve_calculated_times(width, height, root_font_size);
        self.resolve_calculated_easings(width, height, root_font_size);
        self.resolve_iterations(width, height, root_font_size);
        let resolve = |length: Length| {
            length
                .resolve_root_font_units(root_font_size)
                .resolve_viewport_units(width, height)
        };
        self.background_position_x = resolve(self.background_position_x.clone());
        self.background_position_y = resolve(self.background_position_y.clone());
        self.object_position =
            self.object_position
                .clone()
                .resolve_relative_units(width, height, root_font_size);
        if let BackgroundSize::Explicit {
            width: background_width,
            height: background_height,
        } = self.background_size.clone()
        {
            self.background_size = BackgroundSize::Explicit {
                width: resolve(background_width),
                height: resolve(background_height),
            };
        }
        self.width = resolve(self.width.clone());
        self.height = resolve(self.height.clone());
        self.min_width = resolve(self.min_width.clone());
        self.min_height = resolve(self.min_height.clone());
        self.max_width = resolve(self.max_width.clone());
        self.max_height = resolve(self.max_height.clone());
        self.margin = self
            .margin
            .clone()
            .resolve_relative_units(width, height, root_font_size);
        self.padding = self
            .padding
            .clone()
            .resolve_relative_units(width, height, root_font_size);
        self.scroll_margin =
            self.scroll_margin
                .clone()
                .resolve_relative_units(width, height, root_font_size);
        self.scroll_padding =
            self.scroll_padding
                .clone()
                .resolve_relative_units(width, height, root_font_size);
        self.border_width =
            self.border_width
                .clone()
                .resolve_relative_units(width, height, root_font_size);
        self.border_radius = resolve(self.border_radius.clone());
        self.border_spacing = self.border_spacing.clone().map(resolve);
        self.top = resolve(self.top.clone());
        self.right = resolve(self.right.clone());
        self.bottom = resolve(self.bottom.clone());
        self.left = resolve(self.left.clone());
        self.flex_basis = resolve(self.flex_basis.clone());
        self.grid_column_gap = resolve(self.grid_column_gap.clone());
        self.grid_row_gap = resolve(self.grid_row_gap.clone());
        self.clip_path
            .resolve_relative_units(width, height, root_font_size);
        self.transform.resolve_root_font_units(root_font_size);
    }
}
