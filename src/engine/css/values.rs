//! Computed CSS value types and inherited/initial style state.

pub(crate) mod animations;
pub(super) mod border_widths;
pub(super) mod borders;
pub(crate) mod calculated_easing;
pub(crate) mod calculated_times;
mod content_alignment;
mod edges;
pub use content_alignment::ContentAlignment;
mod length;
mod line_height;
pub(crate) mod math;
pub(crate) mod scalars;
pub(crate) mod text_spacing;
pub(crate) use line_height::LineHeight;
mod overflow;
pub use overflow::Overflow;
mod object;
pub use object::{ObjectFit, ObjectPosition};
pub(crate) mod font_features;
pub(crate) mod grid_tracks;
mod text_direction;
pub use font_features::{FontFeatures, FontKerning};
mod font_variants;
pub use font_variants::{FontLigatures, FontNumeric, FontVariants};
mod text_transform;
pub use text_direction::Direction;
pub use text_transform::TextTransform;
pub(super) mod transitions;
mod vertical_align;
mod viewport;
pub use vertical_align::VerticalAlign;

use super::*;
mod color;
pub use color::Color;

#[derive(Debug, Clone, PartialEq)]
pub enum Length {
    Auto,
    Px(f32),
    Percent(f32),
    Em(f32),
    Rem(f32),
    Vw(f32),
    Vh(f32),
    Vmin(f32),
    Vmax(f32),
    Calc {
        px: f32,
        percent: f32,
        em: f32,
        rem: f32,
        vw: f32,
        vh: f32,
        vmin: f32,
        vmax: f32,
    },
    Math(std::sync::Arc<math::Expression>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Edges {
    pub top: Length,
    pub right: Length,
    pub bottom: Length,
    pub left: Length,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ResolvedEdges {
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
    pub left: f32,
}

impl ResolvedEdges {
    pub fn horizontal(self) -> f32 {
        self.left + self.right
    }

    pub fn vertical(self) -> f32 {
        self.top + self.bottom
    }
}

mod display;
pub use display::Display;
mod truncation;
pub use truncation::{BoxOrient, LineClamp, TextOverflow};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Position {
    Sticky,
    Static,
    Relative,
    Absolute,
    Fixed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlexDirection {
    Row,
    RowReverse,
    Column,
    ColumnReverse,
}

impl FlexDirection {
    pub(crate) fn is_row(self) -> bool {
        matches!(self, Self::Row | Self::RowReverse)
    }

    pub(crate) fn is_reverse(self) -> bool {
        matches!(self, Self::RowReverse | Self::ColumnReverse)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JustifyContent {
    Start,
    End,
    Center,
    SpaceBetween,
    SpaceAround,
    SpaceEvenly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlignItems {
    Stretch,
    Start,
    End,
    Center,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextAlign {
    Start,
    Center,
    End,
    Left,
    Right,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WhiteSpace {
    Normal,
    NoWrap,
    Pre,
    PreWrap,
}

impl WhiteSpace {
    pub(crate) fn preserves_spaces(self) -> bool {
        matches!(self, Self::Pre | Self::PreWrap)
    }

    pub(crate) fn wraps(self) -> bool {
        matches!(self, Self::Normal | Self::PreWrap)
    }
}

mod floats;
pub use floats::{Clear, Float};
mod aspect_ratio;
pub use aspect_ratio::AspectRatio;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BoxSizing {
    ContentBox,
    BorderBox,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListStyleType {
    None,
    Disc,
}

#[derive(Debug, Clone, PartialEq)]
pub enum BackgroundSize {
    Auto,
    Contain,
    Cover,
    Explicit { width: Length, height: Length },
}

#[derive(Debug, Clone, PartialEq)]
pub struct ComputedStyle {
    pub(crate) scalar_calculations: scalars::ScalarCalculations,
    pub(crate) transition: transitions::TransitionSettings,
    pub(crate) animation: Arc<animations::AnimationSettings>,
    pub generated_content: GeneratedContent,
    pub display: Display,
    pub position: Position,
    pub z_index: Option<i32>,
    pub float: Float,
    pub clear: Clear,
    pub color: Color,
    pub background_color: Color,
    pub background_image: Option<String>,
    pub mask_image: Option<String>,
    pub background_repeat_x: bool,
    pub background_repeat_y: bool,
    pub background_position_x: Length,
    pub background_position_y: Length,
    pub background_size: BackgroundSize,
    pub object_fit: ObjectFit,
    pub object_position: ObjectPosition,
    pub aspect_ratio: AspectRatio,
    pub font_size: f32,
    pub(crate) root_font_size: f32,
    pub font_weight: u16,
    pub italic: bool,
    pub font_family: String,
    pub font_features: FontFeatures,
    pub font_kerning: FontKerning,
    pub font_ligatures: FontLigatures,
    pub font_numeric: FontNumeric,
    pub letter_spacing: f32,
    pub(crate) letter_spacing_normal: bool,
    pub(crate) pending_spacing: text_spacing::PendingSpacing,
    pub word_spacing: f32,
    pub line_height: f32,
    pub(crate) line_height_value: LineHeight,
    pub text_align: TextAlign,
    pub direction: Direction,
    pub text_transform: TextTransform,
    pub white_space: WhiteSpace,
    pub text_decoration_underline: bool,
    pub text_overflow: TextOverflow,
    pub line_clamp: LineClamp,
    pub box_orient: BoxOrient,
    /// Whether `display` was authored as the legacy `-webkit-box` value. The
    /// box still lays out as ordinary block flow; this flag only records the
    /// authored value so legacy line-clamp activation can distinguish it from
    /// a plain `display: block` without adopting the modern flexbox model.
    pub legacy_webkit_box: bool,
    pub width: Length,
    pub height: Length,
    pub min_width: Length,
    pub min_height: Length,
    pub max_width: Length,
    pub max_height: Length,
    pub margin: Edges,
    pub padding: Edges,
    pub scroll_margin: Edges,
    pub scroll_padding: Edges,
    pub border_width: Edges,
    /// Top, right, bottom, left; None is the computed `currentcolor` keyword.
    pub border_colors: [Option<Color>; 4],
    pub border_radius: Length,
    pub top: Length,
    pub right: Length,
    pub bottom: Length,
    pub left: Length,
    pub visibility: bool,
    pub(crate) content_visibility_hidden: bool,
    pub pointer_events: bool,
    pub opacity: f32,
    pub(crate) clip_path: clip_path::ClipPath,
    pub(crate) transform: transform::TransformList,
    pub(crate) perspective_non_none: bool,
    pub(crate) filter_non_none: bool,
    pub(crate) transform_style_preserve_3d: bool,
    pub(crate) contain_layout_or_paint: bool,
    pub(crate) will_change_containing_block: bool,
    pub overflow_hidden: bool,
    pub(crate) overflow: overflow::OverflowAxes,
    pub justify_content_end: bool,
    pub align_items_center: bool,
    pub flex_direction: FlexDirection,
    pub justify_content: JustifyContent,
    pub align_items: AlignItems,
    pub align_content: ContentAlignment,
    pub justify_self: AlignItems,
    pub flex_wrap: bool,
    pub flex_grow: f32,
    pub flex_shrink: f32,
    pub flex_basis: Length,
    pub box_sizing: BoxSizing,
    pub border_collapse: bool,
    /// Horizontal and vertical spacing in the separated table border model.
    pub border_spacing: [Length; 2],
    pub caption_side_bottom: bool,
    pub vertical_align: VerticalAlign,
    pub list_style_type: ListStyleType,
    pub grid_template_columns: String,
    pub grid_template_rows: String,
    pub grid_template_areas: String,
    pub grid_column_gap: Length,
    pub grid_row_gap: Length,
    pub grid_area_name: Option<String>,
    pub grid_column_start: Option<usize>,
    pub grid_column_end: Option<usize>,
    pub grid_row_start: Option<usize>,
    pub grid_row_end: Option<usize>,
    pub(super) custom_properties: Arc<HashMap<String, String>>,
}

mod computed_defaults;

impl ComputedStyle {
    pub(crate) fn establishes_fixed_position_containing_block(&self) -> bool {
        !self.transform.is_none()
            || self.perspective_non_none
            || self.filter_non_none
            || self.transform_style_preserve_3d
            || self.contain_layout_or_paint
            || self.will_change_containing_block
    }
}
