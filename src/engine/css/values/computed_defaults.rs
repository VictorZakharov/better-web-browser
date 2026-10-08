//! Initial values and inherited computed state, separate from the value type facade.
use super::*;

impl ComputedStyle {
    pub(crate) fn initial() -> Self {
        Self {
            scalar_calculations: scalars::ScalarCalculations::default(),
            transition: transitions::TransitionSettings::default(),
            animation: animations::AnimationSettings::initial(),
            generated_content: GeneratedContent::Normal,
            display: Display::Inline,
            position: Position::Static,
            z_index: None,
            float: Float::None,
            clear: Clear::None,
            color: Color::BLACK,
            background_color: Color::TRANSPARENT,
            background_image: None,
            mask_image: None,
            background_repeat_x: true,
            background_repeat_y: true,
            background_position_x: Length::Percent(0.0),
            background_position_y: Length::Percent(0.0),
            background_size: BackgroundSize::Auto,
            object_fit: ObjectFit::Fill,
            object_position: ObjectPosition::default(),
            aspect_ratio: AspectRatio::Auto,
            font_size: 16.0,
            root_font_size: 16.0,
            font_weight: 400,
            italic: false,
            font_family: "Arial".to_string(),
            font_features: FontFeatures::default(),
            font_kerning: FontKerning::Auto,
            font_ligatures: FontLigatures::default(),
            font_numeric: FontNumeric::default(),
            letter_spacing: 0.0,
            letter_spacing_normal: true,
            pending_spacing: text_spacing::PendingSpacing::default(),
            word_spacing: 0.0,
            line_height: 19.2,
            line_height_value: LineHeight::Normal,
            text_align: TextAlign::Start,
            direction: Direction::Ltr,
            text_transform: TextTransform::None,
            white_space: WhiteSpace::Normal,
            text_decoration_underline: false,
            text_overflow: TextOverflow::Clip,
            line_clamp: LineClamp::None,
            box_orient: BoxOrient::Horizontal,
            legacy_webkit_box: false,
            width: Length::Auto,
            height: Length::Auto,
            min_width: Length::Auto,
            min_height: Length::Auto,
            max_width: Length::Auto,
            max_height: Length::Auto,
            margin: Edges::ZERO,
            padding: Edges::ZERO,
            scroll_margin: Edges::ZERO,
            scroll_padding: Edges {
                top: Length::Auto,
                right: Length::Auto,
                bottom: Length::Auto,
                left: Length::Auto,
            },
            border_width: Edges::ZERO,
            border_colors: [None; 4],
            border_radius: Length::Px(0.0),
            top: Length::Auto,
            right: Length::Auto,
            bottom: Length::Auto,
            left: Length::Auto,
            visibility: true,
            content_visibility_hidden: false,
            pointer_events: true,
            opacity: 1.0,
            clip_path: clip_path::ClipPath::default(),
            transform: transform::TransformList::default(),
            perspective_non_none: false,
            filter_non_none: false,
            transform_style_preserve_3d: false,
            contain_layout_or_paint: false,
            will_change_containing_block: false,
            overflow_hidden: false,
            overflow: overflow::OverflowAxes::default(),
            justify_content_end: false,
            align_items_center: false,
            flex_direction: FlexDirection::Row,
            justify_content: JustifyContent::Start,
            align_items: AlignItems::Stretch,
            align_content: ContentAlignment::default(),
            justify_self: AlignItems::Stretch,
            flex_wrap: false,
            flex_grow: 0.0,
            flex_shrink: 1.0,
            flex_basis: Length::Auto,
            box_sizing: BoxSizing::ContentBox,
            border_collapse: false,
            border_spacing: [const { Length::Px(0.0) }; 2],
            caption_side_bottom: false,
            vertical_align: VerticalAlign::Baseline,
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

    pub(crate) fn inherit_from(parent: Option<&Self>) -> Self {
        let mut style = Self::initial();
        if let Some(parent) = parent {
            style.color = parent.color;
            style.font_size = parent.font_size;
            style.root_font_size = parent.root_font_size;
            style.font_weight = parent.font_weight;
            style.italic = parent.italic;
            style.font_family.clone_from(&parent.font_family);
            style.font_features.clone_from(&parent.font_features);
            style.font_kerning = parent.font_kerning;
            style.font_ligatures = parent.font_ligatures;
            style.font_numeric = parent.font_numeric;
            style.letter_spacing = parent.letter_spacing;
            style.letter_spacing_normal = parent.letter_spacing_normal;
            style.word_spacing = parent.word_spacing;
            style.line_height = parent.line_height;
            style.line_height_value = parent.line_height_value.clone();
            style.text_align = parent.text_align;
            style.direction = parent.direction;
            style.text_transform = parent.text_transform;
            style.white_space = parent.white_space;
            style.border_collapse = parent.border_collapse;
            style.border_spacing = parent.border_spacing.clone();
            style.caption_side_bottom = parent.caption_side_bottom;
            style.list_style_type = parent.list_style_type;
            style.visibility = parent.visibility;
            style.pointer_events = parent.pointer_events;
            style.custom_properties = Arc::clone(&parent.custom_properties);
        }
        style
    }
}
