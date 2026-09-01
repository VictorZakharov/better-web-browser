//! Comparisons that separate geometry-affecting style changes from paint-only changes.

use super::ComputedStyle;

impl ComputedStyle {
    pub(crate) fn layout_equivalent(&self, other: &Self) -> bool {
        self.generated_content == other.generated_content
            && self.display == other.display
            && self.position == other.position
            && self.float == other.float
            && self.font_size == other.font_size
            && self.font_weight == other.font_weight
            && self.italic == other.italic
            && self.font_family == other.font_family
            && self.letter_spacing == other.letter_spacing
            && self.word_spacing == other.word_spacing
            && self.line_height == other.line_height
            && self.text_align == other.text_align
            && self.white_space == other.white_space
            && self.width == other.width
            && self.height == other.height
            && self.min_width == other.min_width
            && self.min_height == other.min_height
            && self.max_width == other.max_width
            && self.max_height == other.max_height
            && self.margin == other.margin
            && self.padding == other.padding
            && self.border_width == other.border_width
            && self.top == other.top
            && self.right == other.right
            && self.bottom == other.bottom
            && self.left == other.left
            && self.transform == other.transform
            && self.perspective_non_none == other.perspective_non_none
            && self.filter_non_none == other.filter_non_none
            && self.transform_style_preserve_3d == other.transform_style_preserve_3d
            && self.contain_layout_or_paint == other.contain_layout_or_paint
            && self.will_change_containing_block == other.will_change_containing_block
            && self.justify_content_end == other.justify_content_end
            && self.align_items_center == other.align_items_center
            && self.flex_direction == other.flex_direction
            && self.justify_content == other.justify_content
            && self.align_items == other.align_items
            && self.justify_self == other.justify_self
            && self.flex_wrap == other.flex_wrap
            && self.flex_grow == other.flex_grow
            && self.flex_shrink == other.flex_shrink
            && self.flex_basis == other.flex_basis
            && self.box_sizing == other.box_sizing
            && self.border_collapse == other.border_collapse
            && self.caption_side_bottom == other.caption_side_bottom
            && self.list_style_type == other.list_style_type
            && self.grid_template_columns == other.grid_template_columns
            && self.grid_template_rows == other.grid_template_rows
            && self.grid_template_areas == other.grid_template_areas
            && self.grid_column_gap == other.grid_column_gap
            && self.grid_row_gap == other.grid_row_gap
            && self.grid_area_name == other.grid_area_name
            && self.grid_column_start == other.grid_column_start
            && self.grid_column_end == other.grid_column_end
            && self.grid_row_start == other.grid_row_start
            && self.grid_row_end == other.grid_row_end
    }
}
