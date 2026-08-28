//! Computed CSS value types and inherited/initial style state.

use super::*;
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

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Length {
    Auto,
    Px(f32),
    Percent(f32),
    Em(f32),
    Vw(f32),
    Vh(f32),
    Vmin(f32),
    Vmax(f32),
    Calc {
        px: f32,
        percent: f32,
        em: f32,
        vw: f32,
        vh: f32,
        vmin: f32,
        vmax: f32,
    },
}

impl Length {
    pub fn resolve(self, basis: f32, font_size: f32) -> Option<f32> {
        match self {
            Self::Auto => None,
            Self::Px(value) => Some(value),
            Self::Percent(value) => Some(basis * value / 100.0),
            Self::Em(value) => Some(font_size * value),
            Self::Vw(value) | Self::Vh(value) | Self::Vmin(value) | Self::Vmax(value) => {
                Some(basis * value / 100.0)
            }
            Self::Calc {
                px,
                percent,
                em,
                vw,
                vh,
                vmin,
                vmax,
            } => Some(
                px + basis * percent / 100.0
                    + font_size * em
                    + basis * vw / 100.0
                    + basis * vh / 100.0
                    + basis * vmin / 100.0
                    + basis * vmax / 100.0,
            ),
        }
    }

    pub(super) fn resolve_viewport_units(self, width: f32, height: f32) -> Self {
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
                vw: 0.0,
                vh: 0.0,
                vmin: 0.0,
                vmax: 0.0,
            },
            value => value,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Edges {
    pub top: Length,
    pub right: Length,
    pub bottom: Length,
    pub left: Length,
}

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

    fn resolve_viewport_units(self, width: f32, height: f32) -> Self {
        Self {
            top: self.top.resolve_viewport_units(width, height),
            right: self.right.resolve_viewport_units(width, height),
            bottom: self.bottom.resolve_viewport_units(width, height),
            left: self.left.resolve_viewport_units(width, height),
        }
    }
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Display {
    None,
    Contents,
    Block,
    Inline,
    InlineBlock,
    InlineFlex,
    Flex,
    Grid,
    Table,
    TableRow,
    TableCell,
}

impl Display {
    pub(crate) const fn css_keyword(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Contents => "contents",
            Self::Block => "block",
            Self::Inline => "inline",
            Self::InlineBlock => "inline-block",
            Self::InlineFlex => "inline-flex",
            Self::Flex => "flex",
            Self::Grid => "grid",
            Self::Table => "table",
            Self::TableRow => "table-row",
            Self::TableCell => "table-cell",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Position {
    Static,
    Relative,
    Absolute,
    Fixed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlexDirection {
    Row,
    Column,
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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WhiteSpace {
    Normal,
    NoWrap,
    Pre,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Float {
    None,
    Left,
    Right,
}

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

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BackgroundSize {
    Auto,
    Contain,
    Cover,
    Explicit { width: Length, height: Length },
}

#[derive(Debug, Clone, PartialEq)]
pub struct ComputedStyle {
    pub display: Display,
    pub position: Position,
    pub float: Float,
    pub color: Color,
    pub background_color: Color,
    pub background_image: Option<String>,
    pub mask_image: Option<String>,
    pub background_repeat_x: bool,
    pub background_repeat_y: bool,
    pub background_position_x: Length,
    pub background_position_y: Length,
    pub background_size: BackgroundSize,
    pub font_size: f32,
    pub font_weight: u16,
    pub italic: bool,
    pub font_family: String,
    pub letter_spacing: f32,
    pub word_spacing: f32,
    pub line_height: f32,
    pub text_align: TextAlign,
    pub white_space: WhiteSpace,
    pub text_decoration_underline: bool,
    pub width: Length,
    pub height: Length,
    pub min_width: Length,
    pub min_height: Length,
    pub max_width: Length,
    pub max_height: Length,
    pub margin: Edges,
    pub padding: Edges,
    pub border_width: Edges,
    pub border_color: Color,
    pub border_radius: Length,
    pub top: Length,
    pub right: Length,
    pub bottom: Length,
    pub left: Length,
    pub visibility: bool,
    pub opacity: f32,
    pub overflow_hidden: bool,
    pub justify_content_end: bool,
    pub align_items_center: bool,
    pub flex_direction: FlexDirection,
    pub justify_content: JustifyContent,
    pub align_items: AlignItems,
    pub justify_self: AlignItems,
    pub flex_wrap: bool,
    pub flex_grow: f32,
    pub flex_shrink: f32,
    pub flex_basis: Length,
    pub box_sizing: BoxSizing,
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

impl ComputedStyle {
    pub(super) fn initial() -> Self {
        Self {
            display: Display::Inline,
            position: Position::Static,
            float: Float::None,
            color: Color::BLACK,
            background_color: Color::TRANSPARENT,
            background_image: None,
            mask_image: None,
            background_repeat_x: true,
            background_repeat_y: true,
            background_position_x: Length::Percent(0.0),
            background_position_y: Length::Percent(0.0),
            background_size: BackgroundSize::Auto,
            font_size: 16.0,
            font_weight: 400,
            italic: false,
            font_family: "Arial".to_string(),
            letter_spacing: 0.0,
            word_spacing: 0.0,
            line_height: 19.2,
            text_align: TextAlign::Start,
            white_space: WhiteSpace::Normal,
            text_decoration_underline: false,
            width: Length::Auto,
            height: Length::Auto,
            min_width: Length::Auto,
            min_height: Length::Auto,
            max_width: Length::Auto,
            max_height: Length::Auto,
            margin: Edges::ZERO,
            padding: Edges::ZERO,
            border_width: Edges::ZERO,
            border_color: Color::BLACK,
            border_radius: Length::Px(0.0),
            top: Length::Auto,
            right: Length::Auto,
            bottom: Length::Auto,
            left: Length::Auto,
            visibility: true,
            opacity: 1.0,
            overflow_hidden: false,
            justify_content_end: false,
            align_items_center: false,
            flex_direction: FlexDirection::Row,
            justify_content: JustifyContent::Start,
            align_items: AlignItems::Stretch,
            justify_self: AlignItems::Stretch,
            flex_wrap: false,
            flex_grow: 0.0,
            flex_shrink: 1.0,
            flex_basis: Length::Auto,
            box_sizing: BoxSizing::ContentBox,
            list_style_type: ListStyleType::Disc,
            grid_template_columns: String::new(),
            grid_template_rows: String::new(),
            grid_template_areas: String::new(),
            grid_column_gap: Length::Px(0.0),
            grid_row_gap: Length::Px(0.0),
            grid_area_name: None,
            grid_column_start: None,
            grid_column_end: None,
            grid_row_start: None,
            grid_row_end: None,
            custom_properties: Arc::new(HashMap::new()),
        }
    }

    pub(super) fn inherit_from(parent: Option<&Self>) -> Self {
        let mut style = Self::initial();
        if let Some(parent) = parent {
            style.color = parent.color;
            style.font_size = parent.font_size;
            style.font_weight = parent.font_weight;
            style.italic = parent.italic;
            style.font_family.clone_from(&parent.font_family);
            style.letter_spacing = parent.letter_spacing;
            style.word_spacing = parent.word_spacing;
            style.line_height = parent.line_height;
            style.text_align = parent.text_align;
            style.white_space = parent.white_space;
            style.list_style_type = parent.list_style_type;
            style.visibility = parent.visibility;
            style.custom_properties = Arc::clone(&parent.custom_properties);
        }
        style
    }

    pub(super) fn resolve_viewport_units(&mut self, width: f32, height: f32) {
        self.background_position_x = self
            .background_position_x
            .resolve_viewport_units(width, height);
        self.background_position_y = self
            .background_position_y
            .resolve_viewport_units(width, height);
        if let BackgroundSize::Explicit {
            width: background_width,
            height: background_height,
        } = self.background_size
        {
            self.background_size = BackgroundSize::Explicit {
                width: background_width.resolve_viewport_units(width, height),
                height: background_height.resolve_viewport_units(width, height),
            };
        }
        self.width = self.width.resolve_viewport_units(width, height);
        self.height = self.height.resolve_viewport_units(width, height);
        self.min_width = self.min_width.resolve_viewport_units(width, height);
        self.min_height = self.min_height.resolve_viewport_units(width, height);
        self.max_width = self.max_width.resolve_viewport_units(width, height);
        self.max_height = self.max_height.resolve_viewport_units(width, height);
        self.margin = self.margin.resolve_viewport_units(width, height);
        self.padding = self.padding.resolve_viewport_units(width, height);
        self.border_width = self.border_width.resolve_viewport_units(width, height);
        self.border_radius = self.border_radius.resolve_viewport_units(width, height);
        self.top = self.top.resolve_viewport_units(width, height);
        self.right = self.right.resolve_viewport_units(width, height);
        self.bottom = self.bottom.resolve_viewport_units(width, height);
        self.left = self.left.resolve_viewport_units(width, height);
        self.flex_basis = self.flex_basis.resolve_viewport_units(width, height);
        self.grid_column_gap = self.grid_column_gap.resolve_viewport_units(width, height);
        self.grid_row_gap = self.grid_row_gap.resolve_viewport_units(width, height);
    }
}
