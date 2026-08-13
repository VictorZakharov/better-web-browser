use super::css::{
    AlignItems, BackgroundSize, BoxSizing, Color, ComputedStyle, Display, FlexDirection, Float,
    JustifyContent, Length, Position, ResolvedEdges, StyleSet, TextAlign, WhiteSpace, parse_length,
};
use super::dom::{Node, NodeData, NodeId, NodeRef};
use super::page::{Page, inline_svg_key};
use crate::navigation::resolve_url;
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct RectF {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl RectF {
    pub fn right(self) -> f32 {
        self.x + self.width
    }

    pub fn bottom(self) -> f32 {
        self.y + self.height
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct FontSpec {
    pub family: String,
    pub size: f32,
    pub weight: u16,
    pub italic: bool,
    pub underline: bool,
}

impl FontSpec {
    fn from_style(style: &ComputedStyle) -> Self {
        Self {
            family: style.font_family.clone(),
            size: style.font_size,
            weight: style.font_weight,
            italic: style.italic,
            underline: style.text_decoration_underline,
        }
    }
}

pub trait TextMeasurer {
    fn measure(&mut self, text: &str, font: &FontSpec) -> (f32, f32);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ControlKind {
    Text,
    TextArea,
    Password,
    Search,
    Select,
    Submit,
    Button,
    Reset,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectOption {
    pub value: String,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ControlSpec {
    pub node_id: NodeId,
    pub rect: RectF,
    pub kind: ControlKind,
    pub name: String,
    pub value: String,
    pub label: String,
    pub options: Vec<SelectOption>,
    pub selected_index: usize,
    pub placeholder: String,
    pub form_id: Option<NodeId>,
    pub background_color: Color,
    pub text_color: Color,
    pub border_color: Color,
    pub border_width: [f32; 4],
    pub border_radius: f32,
    pub padding: [f32; 4],
    pub font: FontSpec,
    pub icon_url: Option<String>,
    pub icon_width: f32,
    pub icon_height: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FormSpec {
    pub node_id: NodeId,
    pub action: String,
    pub method: String,
    pub hidden_fields: Vec<(String, String)>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum DisplayItem {
    SolidRect {
        rect: RectF,
        color: Color,
        radius: f32,
    },
    BorderRect {
        rect: RectF,
        widths: [f32; 4],
        color: Color,
        radius: f32,
    },
    Text {
        rect: RectF,
        text: String,
        font: FontSpec,
        color: Color,
        link: Option<String>,
    },
    Image {
        rect: RectF,
        url: String,
        alt: String,
        tint: Option<Color>,
    },
    BackgroundImage {
        clip_rect: RectF,
        tile_rect: RectF,
        url: String,
        repeat_x: bool,
        repeat_y: bool,
    },
    Control(Box<ControlSpec>),
}

#[derive(Debug, Default)]
pub struct LayoutOutput {
    pub items: Vec<DisplayItem>,
    pub content_height: f32,
    pub background: Color,
    pub forms: HashMap<NodeId, FormSpec>,
}

pub fn layout_page<M: TextMeasurer>(
    page: &Page,
    viewport_width: f32,
    viewport_height: f32,
    measurer: &mut M,
) -> LayoutOutput {
    layout_page_with_style_viewport(
        page,
        viewport_width,
        viewport_height,
        viewport_width,
        measurer,
    )
}

/// Lays out a page when the CSS media viewport and content area have different widths.
/// Classic scrollbars occupy content space but remain part of the media-query viewport.
pub fn layout_page_with_style_viewport<M: TextMeasurer>(
    page: &Page,
    viewport_width: f32,
    viewport_height: f32,
    style_viewport_width: f32,
    measurer: &mut M,
) -> LayoutOutput {
    let computed_styles;
    let cached_styles = page.cached_style(style_viewport_width);
    let styles = if let Some(cached_styles) = cached_styles {
        cached_styles
    } else {
        computed_styles = page.style(style_viewport_width);
        &computed_styles
    };
    let mut engine = LayoutEngine {
        page,
        styles,
        measurer,
        measurement_cache: HashMap::new(),
        inline_box_cache: HashMap::new(),
        viewport: RectF {
            x: 0.0,
            y: 0.0,
            width: viewport_width.max(1.0),
            height: viewport_height.max(1.0),
        },
        output: LayoutOutput {
            items: Vec::new(),
            content_height: viewport_height,
            background: Color::WHITE,
            forms: collect_forms(page),
        },
    };

    let root = page
        .dom
        .elements_named("body")
        .next()
        .or_else(|| page.dom.elements_named("html").next())
        .unwrap_or_else(|| page.dom.document.clone());
    if let Some(body_style) = engine.styles.styles.get(&node_id(&root))
        && body_style.background_color.alpha > 0
    {
        engine.output.background = body_style.background_color.composite_over(Color::WHITE);
    }
    let metrics = engine.layout_block(&root, 0.0, 0.0, viewport_width.max(1.0));
    engine.output.content_height = metrics.bottom.max(viewport_height);
    engine.output
}

struct LayoutEngine<'a, M> {
    page: &'a Page,
    styles: &'a StyleSet,
    measurer: &'a mut M,
    measurement_cache: HashMap<(usize, bool), CachedAtomMeasurement>,
    inline_box_cache: HashMap<usize, InlineBoxMetrics>,
    viewport: RectF,
    output: LayoutOutput,
}

#[derive(Debug, Clone, Copy)]
struct BlockMetrics {
    bottom: f32,
}

impl<M: TextMeasurer> LayoutEngine<'_, M> {
    fn layout_block(
        &mut self,
        node: &NodeRef,
        containing_x: f32,
        y: f32,
        containing_width: f32,
    ) -> BlockMetrics {
        let style = self.styles.get(node).clone();
        if style.display == Display::None || !style.visibility {
            return BlockMetrics { bottom: y };
        }
        let block_control = input_control_data(node);

        let margins = style.margin.resolve(containing_width, style.font_size);
        let borders = style
            .border_width
            .resolve(containing_width, style.font_size);
        let padding = style.padding.resolve(containing_width, style.font_size);
        let horizontal_insets = padding.horizontal() + borders.horizontal();
        let available_width = (containing_width - margins.horizontal()).max(0.0);
        let mut border_box_width = resolve_outer_size(
            style.width,
            containing_width,
            style.font_size,
            horizontal_insets,
            style.box_sizing,
        )
        .unwrap_or(available_width);
        if let Some(maximum) = resolve_outer_size(
            style.max_width,
            containing_width,
            style.font_size,
            horizontal_insets,
            style.box_sizing,
        ) {
            border_box_width = border_box_width.min(maximum);
        }
        if let Some(minimum) = resolve_outer_size(
            style.min_width,
            containing_width,
            style.font_size,
            horizontal_insets,
            style.box_sizing,
        ) {
            border_box_width = border_box_width.max(minimum);
        }
        border_box_width = border_box_width.max(0.0);

        let auto_left = style.margin.left == Length::Auto;
        let auto_right = style.margin.right == Length::Auto;
        let mut x = containing_x + margins.left;
        if auto_left && auto_right && border_box_width < containing_width {
            x = containing_x + (containing_width - border_box_width) / 2.0;
        } else if style.float == Float::Right || auto_left {
            x = containing_x + containing_width - border_box_width - margins.right;
        }

        let mut border_y = y + margins.top;
        if matches!(style.position, Position::Absolute | Position::Fixed) {
            let (positioning_x, positioning_y, positioning_width, positioning_height) =
                if style.position == Position::Fixed {
                    (
                        self.viewport.x,
                        self.viewport.y,
                        self.viewport.width,
                        self.viewport.height,
                    )
                } else {
                    (containing_x, y, containing_width, self.viewport.height)
                };
            let left = style.left.resolve(positioning_width, style.font_size);
            let right = style.right.resolve(positioning_width, style.font_size);
            if let Some(left) = left {
                x = positioning_x + left;
                if right.is_some()
                    && auto_left
                    && auto_right
                    && border_box_width < positioning_width
                {
                    x = positioning_x + (positioning_width - border_box_width) / 2.0;
                }
            } else if let Some(right) = right {
                x = positioning_x + positioning_width - border_box_width - right;
            }
            if let Some(top) = style.top.resolve(positioning_height, style.font_size) {
                border_y = positioning_y + top;
            } else if let Some(bottom) = style.bottom.resolve(positioning_height, style.font_size) {
                border_y = positioning_y + positioning_height - bottom;
            }
        } else if style.position == Position::Relative {
            if let Some(left) = style.left.resolve(containing_width, style.font_size) {
                x += left;
            } else if let Some(right) = style.right.resolve(containing_width, style.font_size) {
                x -= right;
            }
            if let Some(top) = style.top.resolve(self.viewport.height, style.font_size) {
                border_y += top;
            } else if let Some(bottom) = style.bottom.resolve(self.viewport.height, style.font_size)
            {
                border_y -= bottom;
            }
        }

        let content_x = x + borders.left + padding.left;
        let content_y = border_y + borders.top + padding.top;
        let content_width =
            (border_box_width - borders.horizontal() - padding.horizontal()).max(0.0);
        let vertical_insets = borders.vertical() + padding.vertical();
        let specified_height = resolve_content_height(
            style.height,
            self.viewport,
            style.font_size,
            vertical_insets,
            style.box_sizing,
        );
        let minimum_height = resolve_content_height(
            style.min_height,
            self.viewport,
            style.font_size,
            vertical_insets,
            style.box_sizing,
        )
        .unwrap_or(0.0);
        let maximum_height = resolve_content_height(
            style.max_height,
            self.viewport,
            style.font_size,
            vertical_insets,
            style.box_sizing,
        );
        let background_index = if style.background_color.alpha > 0 {
            let index = self.output.items.len();
            self.output.items.push(DisplayItem::SolidRect {
                rect: RectF {
                    x,
                    y: border_y,
                    width: border_box_width,
                    height: 0.0,
                },
                color: self.effective_background_color(node),
                radius: 0.0,
            });
            Some(index)
        } else {
            None
        };
        let background_image_index = style.background_image.as_ref().map(|url| {
            let index = self.output.items.len();
            self.output.items.push(DisplayItem::BackgroundImage {
                clip_rect: RectF {
                    x,
                    y: border_y,
                    width: border_box_width,
                    height: 0.0,
                },
                tile_rect: RectF::default(),
                url: url.clone(),
                repeat_x: style.background_repeat_x,
                repeat_y: style.background_repeat_y,
            });
            index
        });

        let collapsed = style.overflow_hidden && maximum_height.is_some_and(|height| height <= 0.0);
        let content_bottom = if collapsed {
            content_y
        } else if let Some((kind, _)) = block_control.as_ref() {
            content_y + default_control_content_height(node, kind, &style)
        } else {
            match style.display {
                Display::Flex => {
                    self.layout_flex(node, content_x, content_y, content_width, &style)
                }
                Display::Grid => {
                    self.layout_grid(node, content_x, content_y, content_width, &style)
                }
                Display::Table => {
                    self.layout_table(node, content_x, content_y, content_width, &style)
                }
                _ => self.layout_block_children(node, content_x, content_y, content_width, &style),
            }
        };
        let natural_content_height = (content_bottom - content_y).max(0.0);
        let mut content_height = specified_height
            .unwrap_or(natural_content_height)
            .max(minimum_height);
        if let Some(maximum_height) = maximum_height {
            content_height = content_height.min(maximum_height);
        }
        let border_box_height =
            borders.top + padding.top + content_height + padding.bottom + borders.bottom;
        let rect = RectF {
            x,
            y: border_y,
            width: border_box_width,
            height: border_box_height,
        };
        let radius = resolve_border_radius(style.border_radius, rect, style.font_size);
        if let Some(index) = background_index
            && let DisplayItem::SolidRect {
                rect: target,
                radius: target_radius,
                ..
            } = &mut self.output.items[index]
        {
            *target = rect;
            *target_radius = radius;
        }
        if let Some(index) = background_image_index
            && let Some(tile_rect) = self.background_tile_rect(&style, rect)
            && let DisplayItem::BackgroundImage {
                clip_rect,
                tile_rect: target_tile,
                ..
            } = &mut self.output.items[index]
        {
            *clip_rect = rect;
            *target_tile = tile_rect;
        }
        if style.border_color.alpha > 0 && (borders.vertical() > 0.0 || borders.horizontal() > 0.0)
        {
            self.output.items.push(DisplayItem::BorderRect {
                rect,
                widths: [borders.top, borders.right, borders.bottom, borders.left],
                color: style
                    .border_color
                    .composite_over(self.effective_background_color(node)),
                radius,
            });
        }
        if let Some((kind, value)) = block_control {
            let icon = self.control_background_icon(&style, rect.width, rect.height);
            let mut label = input_control_label(node, kind, &value);
            if icon.is_some() && value.is_empty() {
                label.clear();
            }
            self.output
                .items
                .push(DisplayItem::Control(Box::new(ControlSpec {
                    node_id: node_id(node),
                    rect,
                    kind,
                    name: node.attr("name").unwrap_or_default(),
                    value,
                    label,
                    options: Vec::new(),
                    selected_index: 0,
                    placeholder: node
                        .attr("placeholder")
                        .or_else(|| node.attr("title"))
                        .unwrap_or_default(),
                    form_id: nearest_form(node).map(|form| node_id(&form)),
                    background_color: self.effective_background_color(node),
                    text_color: style.color,
                    border_color: style
                        .border_color
                        .composite_over(self.effective_background_color(node)),
                    border_width: [borders.top, borders.right, borders.bottom, borders.left],
                    border_radius: radius,
                    padding: [padding.top, padding.right, padding.bottom, padding.left],
                    font: FontSpec::from_style(&style),
                    icon_url: icon.as_ref().map(|(url, _, _)| url.clone()),
                    icon_width: icon.as_ref().map(|(_, width, _)| *width).unwrap_or(0.0),
                    icon_height: icon.as_ref().map(|(_, _, height)| *height).unwrap_or(0.0),
                })));
        }

        let flow_bottom = border_y + border_box_height + margins.bottom;
        BlockMetrics {
            bottom: if matches!(style.position, Position::Absolute | Position::Fixed) {
                y
            } else {
                flow_bottom
            },
        }
    }

    fn layout_block_children(
        &mut self,
        node: &NodeRef,
        x: f32,
        mut y: f32,
        width: f32,
        style: &ComputedStyle,
    ) -> f32 {
        let positioning_y = y;
        let mut atoms = Vec::new();
        let mut pending_space = false;
        let mut left_float_width = 0.0_f32;
        let mut right_float_width = 0.0_f32;
        let mut float_bottom = y;
        if node.tag_name() == Some("li") {
            atoms.push(InlineAtom::Text {
                text: "• ".into(),
                font: FontSpec::from_style(style),
                color: style.color,
                link: None,
                line_height: style.line_height,
                no_wrap: false,
            });
        }
        for child in node.children.borrow().iter() {
            let child_style = self.styles.get(child);
            if is_block_level(child_style.display)
                && child_style.float != Float::None
                && !matches!(child_style.position, Position::Absolute | Position::Fixed)
            {
                let remaining_width = (width - left_float_width - right_float_width).max(0.0);
                let float_width = self
                    .flex_item_basis(child, child_style, remaining_width)
                    .clamp(0.0, remaining_width);
                let float_x = if child_style.float == Float::Right {
                    x + width - right_float_width - float_width
                } else {
                    x + left_float_width
                };
                let metrics = self.layout_block(child, float_x, y, float_width);
                float_bottom = float_bottom.max(metrics.bottom);
                if child_style.float == Float::Right {
                    right_float_width += float_width;
                } else {
                    left_float_width += float_width;
                }
            } else if is_block_level(child_style.display)
                && !matches!(child_style.position, Position::Absolute | Position::Fixed)
            {
                if !atoms.is_empty() {
                    y = self.layout_inline_atoms(
                        &atoms,
                        x + left_float_width,
                        y,
                        (width - left_float_width - right_float_width).max(0.0),
                        style.text_align,
                        style.line_height,
                    );
                    atoms.clear();
                    pending_space = false;
                }
                y = y.max(float_bottom);
                y = self.layout_block(child, x, y, width).bottom;
            } else if is_block_level(child_style.display) {
                self.layout_block(child, x, positioning_y, width);
            } else {
                self.collect_inline(child, None, &mut atoms, &mut pending_space, true);
            }
        }
        if !atoms.is_empty() {
            y = self.layout_inline_atoms(
                &atoms,
                x + left_float_width,
                y,
                (width - left_float_width - right_float_width).max(0.0),
                style.text_align,
                style.line_height,
            );
        }
        y.max(float_bottom)
    }

    fn layout_flex(
        &mut self,
        node: &NodeRef,
        x: f32,
        y: f32,
        width: f32,
        style: &ComputedStyle,
    ) -> f32 {
        let has_direct_text = node.children.borrow().iter().any(
            |child| matches!(&child.data, NodeData::Text(text) if !text.borrow().trim().is_empty()),
        );
        let element_children = node
            .children
            .borrow()
            .iter()
            .filter(|child| {
                child.element().is_some()
                    && self.styles.get(child).display != Display::None
                    && self.styles.get(child).visibility
                    && !style_collapses_overflow(self.styles.get(child), self.viewport)
            })
            .cloned()
            .collect::<Vec<_>>();
        if element_children.is_empty() || has_direct_text {
            return self.layout_flattened_flex_content(node, x, y, width, style);
        }

        for child in element_children.iter().filter(|child| {
            matches!(
                self.styles.get(child).position,
                Position::Absolute | Position::Fixed
            )
        }) {
            self.layout_block(child, x, y, width);
        }
        let items = element_children
            .into_iter()
            .filter(|child| {
                !matches!(
                    self.styles.get(child).position,
                    Position::Absolute | Position::Fixed
                )
            })
            .map(|child| {
                let child_style = self.styles.get(&child).clone();
                FlexItem {
                    basis: self.flex_item_basis(&child, &child_style, width),
                    grow: child_style.flex_grow,
                    shrink: child_style.flex_shrink,
                    margin_start_auto: child_style.margin.left == Length::Auto,
                    margin_end_auto: child_style.margin.right == Length::Auto,
                    node: child,
                }
            })
            .collect::<Vec<_>>();
        if items.is_empty() {
            return y;
        }

        match style.flex_direction {
            FlexDirection::Column => self.layout_flex_column(&items, x, y, width, style),
            FlexDirection::Row => self.layout_flex_rows(&items, x, y, width, style),
        }
    }

    fn layout_flattened_flex_content(
        &mut self,
        node: &NodeRef,
        x: f32,
        y: f32,
        width: f32,
        style: &ComputedStyle,
    ) -> f32 {
        let mut atoms = Vec::new();
        let mut pending_space = false;
        for child in node.children.borrow().iter() {
            self.collect_inline(child, None, &mut atoms, &mut pending_space, false);
        }
        let alignment = if style.justify_content_end || style.float == Float::Right {
            TextAlign::End
        } else {
            style.text_align
        };
        self.layout_inline_atoms(&atoms, x, y, width, alignment, style.line_height)
    }

    fn flex_item_basis(
        &mut self,
        node: &NodeRef,
        style: &ComputedStyle,
        available_width: f32,
    ) -> f32 {
        let margin = style.margin.resolve(available_width, style.font_size);
        let border = style.border_width.resolve(available_width, style.font_size);
        let padding = style.padding.resolve(available_width, style.font_size);
        let insets = border.horizontal() + padding.horizontal();
        let specified = if style.flex_basis != Length::Auto {
            resolve_outer_size(
                style.flex_basis,
                available_width,
                style.font_size,
                insets,
                style.box_sizing,
            )
        } else {
            resolve_outer_size(
                style.width,
                available_width,
                style.font_size,
                insets,
                style.box_sizing,
            )
        };
        let intrinsic_width = if specified.is_some() {
            0.0
        } else {
            let mut atoms = Vec::new();
            let mut pending_space = false;
            for child in node.children.borrow().iter() {
                self.collect_inline(child, None, &mut atoms, &mut pending_space, false);
            }
            self.begin_inline_measurement_context();
            let mut intrinsic_width = 0.0_f32;
            let mut current_line = 0.0_f32;
            let mut line_start = true;
            for atom in &atoms {
                if matches!(atom, InlineAtom::Break) {
                    intrinsic_width = intrinsic_width.max(current_line);
                    current_line = 0.0;
                    line_start = true;
                } else {
                    current_line += self.measure_atom(atom, line_start).width;
                    line_start = false;
                }
            }
            intrinsic_width.max(current_line)
        };
        let mut basis =
            specified.unwrap_or(intrinsic_width + insets).max(0.0) + margin.horizontal();
        if let Some(minimum) = resolve_outer_size(
            style.min_width,
            available_width,
            style.font_size,
            insets,
            style.box_sizing,
        ) {
            basis = basis.max(minimum + margin.horizontal());
        }
        if let Some(maximum) = resolve_outer_size(
            style.max_width,
            available_width,
            style.font_size,
            insets,
            style.box_sizing,
        ) {
            basis = basis.min(maximum + margin.horizontal());
        }
        basis
    }

    fn layout_flex_column(
        &mut self,
        items: &[FlexItem],
        x: f32,
        mut y: f32,
        width: f32,
        style: &ComputedStyle,
    ) -> f32 {
        let gap = style
            .grid_row_gap
            .resolve(width, style.font_size)
            .unwrap_or(0.0)
            .max(0.0);
        for (index, item) in items.iter().enumerate() {
            y = self.layout_flex_item(&item.node, x, y, width).bottom;
            if index + 1 < items.len() {
                y += gap;
            }
        }
        y
    }

    fn layout_flex_rows(
        &mut self,
        items: &[FlexItem],
        x: f32,
        y: f32,
        width: f32,
        style: &ComputedStyle,
    ) -> f32 {
        let gap = style
            .grid_column_gap
            .resolve(width, style.font_size)
            .unwrap_or(0.0)
            .max(0.0);
        let mut lines = Vec::<Vec<FlexItem>>::new();
        let mut current = Vec::new();
        let mut current_width = 0.0_f32;
        for item in items {
            let next_width = if current.is_empty() {
                item.basis
            } else {
                current_width + gap + item.basis
            };
            if style.flex_wrap && !current.is_empty() && next_width > width {
                lines.push(std::mem::take(&mut current));
                current_width = 0.0;
            }
            if !current.is_empty() {
                current_width += gap;
            }
            current_width += item.basis;
            current.push(item.clone());
        }
        if !current.is_empty() {
            lines.push(current);
        }

        let row_gap = style
            .grid_row_gap
            .resolve(width, style.font_size)
            .unwrap_or(0.0)
            .max(0.0);
        let mut cursor_y = y;
        let line_count = lines.len();
        for (index, line) in lines.iter().enumerate() {
            cursor_y = self.layout_flex_row_line(line, x, cursor_y, width, gap, style);
            if index + 1 < line_count {
                cursor_y += row_gap;
            }
        }
        cursor_y
    }

    fn layout_flex_row_line(
        &mut self,
        items: &[FlexItem],
        x: f32,
        y: f32,
        width: f32,
        base_gap: f32,
        style: &ComputedStyle,
    ) -> f32 {
        let gap_width = base_gap * items.len().saturating_sub(1) as f32;
        let mut sizes = items.iter().map(|item| item.basis).collect::<Vec<_>>();
        let basis_sum = sizes.iter().sum::<f32>();
        let free = width - gap_width - basis_sum;
        if free > 0.0 {
            let total_grow = items.iter().map(|item| item.grow).sum::<f32>();
            if total_grow > 0.0 {
                for (size, item) in sizes.iter_mut().zip(items) {
                    *size += free * item.grow / total_grow;
                }
            }
        } else if free < 0.0 {
            let total_shrink = items
                .iter()
                .map(|item| item.shrink * item.basis)
                .sum::<f32>();
            if total_shrink > 0.0 {
                for (size, item) in sizes.iter_mut().zip(items) {
                    let shrink = -free * item.shrink * item.basis / total_shrink;
                    *size = (*size - shrink).max(1.0);
                }
            }
        }

        let unused = (width - gap_width - sizes.iter().sum::<f32>()).max(0.0);
        let automatic_margin_count = items
            .iter()
            .map(|item| item.margin_start_auto as usize + item.margin_end_auto as usize)
            .sum::<usize>();
        let automatic_margin = if automatic_margin_count > 0 {
            unused / automatic_margin_count as f32
        } else {
            0.0
        };
        let justify_space = if automatic_margin_count > 0 {
            0.0
        } else {
            unused
        };
        let (offset, extra_gap) = match style.justify_content {
            JustifyContent::End => (justify_space, 0.0),
            JustifyContent::Center => (justify_space / 2.0, 0.0),
            JustifyContent::SpaceBetween if items.len() > 1 => {
                (0.0, justify_space / (items.len() - 1) as f32)
            }
            JustifyContent::SpaceAround => {
                let share = justify_space / items.len() as f32;
                (share / 2.0, share)
            }
            JustifyContent::SpaceEvenly => {
                let share = justify_space / (items.len() + 1) as f32;
                (share, share)
            }
            _ => (0.0, 0.0),
        };

        let mut cursor_x = x + offset;
        let mut painted = Vec::with_capacity(items.len());
        let mut row_height = 0.0_f32;
        for (index, (item, item_width)) in items.iter().zip(sizes).enumerate() {
            if item.margin_start_auto {
                cursor_x += automatic_margin;
            }
            let output_start = self.output.items.len();
            let metrics = self.layout_flex_item(&item.node, cursor_x, y, item_width.max(1.0));
            let output_end = self.output.items.len();
            let item_height = (metrics.bottom - y).max(0.0);
            row_height = row_height.max(item_height);
            painted.push((output_start, output_end, item_height));
            cursor_x += item_width;
            if item.margin_end_auto {
                cursor_x += automatic_margin;
            }
            if index + 1 < items.len() {
                cursor_x += base_gap + extra_gap;
            }
        }

        let cross_size = resolve_height_value(style.height, self.viewport, style.font_size)
            .unwrap_or(row_height)
            .max(row_height);
        for (start, end, item_height) in painted {
            let offset_y = match style.align_items {
                AlignItems::Center => (cross_size - item_height) / 2.0,
                AlignItems::End => cross_size - item_height,
                AlignItems::Stretch | AlignItems::Start => 0.0,
            };
            if offset_y > 0.0 {
                translate_display_items(&mut self.output.items[start..end], 0.0, offset_y);
            }
        }
        y + cross_size
    }

    fn layout_flex_item(&mut self, node: &NodeRef, x: f32, y: f32, width: f32) -> BlockMetrics {
        let tag = node.tag_name().unwrap_or_default();
        if !matches!(
            tag,
            "img" | "image" | "input" | "textarea" | "button" | "svg"
        ) {
            return self.layout_block(node, x, y, width);
        }

        let mut style = self.styles.get(node).clone();
        let margin = style.margin.resolve(width, style.font_size);
        let border = style.border_width.resolve(width, style.font_size);
        let padding = style.padding.resolve(width, style.font_size);
        let border_box_width = (width - margin.horizontal()).max(1.0);
        style.width = Length::Px(if style.box_sizing == BoxSizing::BorderBox {
            border_box_width
        } else {
            (border_box_width - border.horizontal() - padding.horizontal()).max(1.0)
        });

        let mut atoms = Vec::new();
        match tag {
            "img" | "image" => self.collect_image(node, &style, None, &mut atoms),
            "input" | "textarea" => self.collect_input(node, &style, &mut atoms),
            "button" => self.collect_button(node, &style, &mut atoms),
            "svg" => self.collect_svg(node, &style, &mut atoms),
            _ => {}
        }
        let bottom =
            self.layout_inline_atoms(&atoms, x, y, width, style.text_align, style.line_height);
        BlockMetrics { bottom }
    }

    fn layout_grid(
        &mut self,
        node: &NodeRef,
        x: f32,
        y: f32,
        width: f32,
        style: &ComputedStyle,
    ) -> f32 {
        let mut column_tracks = parse_grid_tracks(&style.grid_template_columns);
        if column_tracks.is_empty() {
            column_tracks.push(GridTrack::Fraction(1.0));
        }
        let column_gap = style
            .grid_column_gap
            .resolve(width, style.font_size)
            .unwrap_or(0.0)
            .max(0.0);
        let row_gap = style
            .grid_row_gap
            .resolve(width, style.font_size)
            .unwrap_or(0.0)
            .max(0.0);
        let column_widths =
            resolve_grid_columns(&column_tracks, width, column_gap, style.font_size);
        let column_count = column_widths.len().max(1);

        let mut placements = Vec::new();
        let mut automatic_index = 0_usize;
        for child in node.children.borrow().iter() {
            if child.element().is_none() {
                continue;
            }
            let child_style = self.styles.get(child);
            if child_style.display == Display::None || !child_style.visibility {
                continue;
            }

            let explicit_column = child_style.grid_column_start.map(|line| line - 1);
            let explicit_row = child_style.grid_row_start.map(|line| line - 1);
            let mut column = explicit_column.unwrap_or(automatic_index % column_count);
            let row = explicit_row.unwrap_or_else(|| {
                if explicit_column.is_some() {
                    automatic_index / column_count
                } else {
                    let automatic_row = automatic_index / column_count;
                    automatic_index += 1;
                    automatic_row
                }
            });
            if explicit_row.is_some() && explicit_column.is_none() {
                column = 0;
            }
            column = column.min(column_count - 1);

            let column_end = child_style
                .grid_column_end
                .map(|line| line.saturating_sub(1))
                .filter(|end| *end > column)
                .unwrap_or(column + 1)
                .min(column_count);
            let row_end = child_style
                .grid_row_end
                .map(|line| line.saturating_sub(1))
                .filter(|end| *end > row)
                .unwrap_or(row + 1);
            placements.push(GridItemPlacement {
                node: child.clone(),
                column,
                column_end,
                row,
                row_end,
            });
        }

        let row_tracks = parse_grid_tracks(&style.grid_template_rows);
        let row_count = placements
            .iter()
            .map(|placement| placement.row_end)
            .max()
            .unwrap_or(0)
            .max(row_tracks.len());
        if row_count == 0 {
            return y;
        }

        let mut cursor_y = y;
        for row in 0..row_count {
            let track_height = row_tracks
                .get(row)
                .map(|track| resolve_grid_row_minimum(track, self.viewport.height, style.font_size))
                .unwrap_or(0.0);
            let mut natural_height = 0.0_f32;
            for placement in placements.iter().filter(|placement| placement.row == row) {
                let cell_x = x
                    + column_widths[..placement.column].iter().sum::<f32>()
                    + column_gap * placement.column as f32;
                let cell_width = column_widths[placement.column..placement.column_end]
                    .iter()
                    .sum::<f32>()
                    + column_gap * placement.column_end.saturating_sub(placement.column + 1) as f32;
                let metrics = self.layout_block(&placement.node, cell_x, cursor_y, cell_width);
                let child_style = self.styles.get(&placement.node);
                if !matches!(child_style.position, Position::Absolute | Position::Fixed) {
                    let span = placement.row_end.saturating_sub(placement.row).max(1) as f32;
                    natural_height =
                        natural_height.max((metrics.bottom - cursor_y).max(0.0) / span);
                }
            }
            cursor_y += track_height.max(natural_height);
            if row + 1 < row_count {
                cursor_y += row_gap;
            }
        }
        cursor_y
    }

    fn layout_table(
        &mut self,
        node: &NodeRef,
        x: f32,
        mut y: f32,
        width: f32,
        _style: &ComputedStyle,
    ) -> f32 {
        let rows = table_rows(node);
        for row in rows {
            let cells = row
                .children
                .borrow()
                .iter()
                .filter(|child| matches!(child.tag_name(), Some("td" | "th")))
                .cloned()
                .collect::<Vec<_>>();
            if cells.is_empty() {
                continue;
            }
            let widths = table_cell_widths(&cells, width, self.styles);
            let mut cell_x = x;
            let mut row_bottom = y;
            for (cell, cell_width) in cells.iter().zip(widths) {
                let cell_style = self.styles.get(cell).clone();
                let bottom = self.layout_block_children(cell, cell_x, y, cell_width, &cell_style);
                row_bottom = row_bottom.max(bottom);
                cell_x += cell_width;
            }
            y = row_bottom;
        }
        y
    }

    fn collect_inline(
        &self,
        node: &NodeRef,
        inherited_link: Option<String>,
        output: &mut Vec<InlineAtom>,
        pending_space: &mut bool,
        honor_block_boundaries: bool,
    ) {
        let style = self.styles.get(node);
        if style.display == Display::None || !style.visibility {
            return;
        }
        match &node.data {
            NodeData::Text(text) => {
                collect_text_atoms(&text.borrow(), style, inherited_link, output, pending_space);
            }
            NodeData::Element(_) => {
                let tag = node.tag_name().unwrap_or_default();
                let link = if tag == "a" {
                    node.attr("href")
                        .and_then(|href| resolve_url(&self.page.source_url, &href))
                        .or(inherited_link)
                } else {
                    inherited_link
                };
                match tag {
                    "br" => {
                        output.push(InlineAtom::Break);
                        *pending_space = false;
                    }
                    "img" | "image" => self.collect_image(node, style, link, output),
                    "input" | "textarea" => self.collect_input(node, style, output),
                    "select" => self.collect_select(node, style, output),
                    "button" => self.collect_button(node, style, output),
                    "svg" => self.collect_svg(node, style, output),
                    _ => {
                        if style.display == Display::InlineBlock
                            || style.margin.left != Length::Px(0.0)
                            || style.margin.right != Length::Px(0.0)
                            || style.padding != super::css::Edges::ZERO
                            || style.border_width != super::css::Edges::ZERO
                            || style.background_color.alpha > 0
                            || style.background_image.is_some()
                        {
                            if *pending_space {
                                output.push(text_atom(" ".into(), style, link.clone()));
                                *pending_space = false;
                            }
                            let mut children = Vec::new();
                            let mut child_pending_space = false;
                            for child in node.children.borrow().iter() {
                                self.collect_inline(
                                    child,
                                    link.clone(),
                                    &mut children,
                                    &mut child_pending_space,
                                    honor_block_boundaries,
                                );
                            }
                            output.push(InlineAtom::InlineBox {
                                children,
                                style: Box::new(style.clone()),
                            });
                        } else if honor_block_boundaries && is_block_level(style.display) {
                            if !output.is_empty()
                                && !matches!(output.last(), Some(InlineAtom::Break))
                            {
                                output.push(InlineAtom::Break);
                            }
                            for child in node.children.borrow().iter() {
                                self.collect_inline(
                                    child,
                                    link.clone(),
                                    output,
                                    pending_space,
                                    honor_block_boundaries,
                                );
                            }
                            if !output.is_empty()
                                && !matches!(output.last(), Some(InlineAtom::Break))
                            {
                                output.push(InlineAtom::Break);
                            }
                        } else {
                            for child in node.children.borrow().iter() {
                                self.collect_inline(
                                    child,
                                    link.clone(),
                                    output,
                                    pending_space,
                                    honor_block_boundaries,
                                );
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }

    fn collect_image(
        &self,
        node: &NodeRef,
        style: &ComputedStyle,
        _link: Option<String>,
        output: &mut Vec<InlineAtom>,
    ) {
        let Some(url) = self.page.image_url(node) else {
            return;
        };
        let intrinsic = self.page.images.get(&url);
        let intrinsic_width = intrinsic.map(|image| image.width as f32).unwrap_or(16.0);
        let intrinsic_height = intrinsic.map(|image| image.height as f32).unwrap_or(16.0);
        let mut width =
            element_length(node, "width", style.width, intrinsic_width, style.font_size);
        let mut height = element_length(
            node,
            "height",
            style.height,
            intrinsic_height,
            style.font_size,
        );
        if style.width != Length::Auto && style.height == Length::Auto && intrinsic_width > 0.0 {
            height = width * intrinsic_height / intrinsic_width;
        } else if style.height != Length::Auto
            && style.width == Length::Auto
            && intrinsic_height > 0.0
        {
            width = height * intrinsic_width / intrinsic_height;
        }
        let margin = style.margin.resolve(self.viewport.width, style.font_size);
        let padding = style.padding.resolve(self.viewport.width, style.font_size);
        let border = style
            .border_width
            .resolve(self.viewport.width, style.font_size);
        output.push(InlineAtom::Image {
            url,
            alt: node.attr("alt").unwrap_or_default(),
            tint: None,
            width: width + margin.horizontal() + padding.horizontal() + border.horizontal(),
            height: height + margin.vertical() + padding.vertical() + border.vertical(),
            inset_x: margin.left + padding.left + border.left,
            inset_y: margin.top + padding.top + border.top,
            image_width: width,
            image_height: height,
        });
    }

    fn collect_svg(&self, node: &NodeRef, style: &ComputedStyle, output: &mut Vec<InlineAtom>) {
        let width = element_length(node, "width", style.width, 24.0, style.font_size);
        let height = element_length(node, "height", style.height, 24.0, style.font_size);
        let key = inline_svg_key(node);
        if self.page.images.contains_key(&key) {
            let margin = style.margin.resolve(self.viewport.width, style.font_size);
            let padding = style.padding.resolve(self.viewport.width, style.font_size);
            output.push(InlineAtom::Image {
                url: key,
                alt: node.attr("aria-label").unwrap_or_default(),
                tint: svg_uses_current_color(node).then_some(style.color),
                width: width + margin.horizontal() + padding.horizontal(),
                height: height + margin.vertical() + padding.vertical(),
                inset_x: margin.left + padding.left,
                inset_y: margin.top + padding.top,
                image_width: width,
                image_height: height,
            });
        } else {
            output.push(InlineAtom::Placeholder { width, height });
        }
    }

    fn collect_input(&self, node: &NodeRef, style: &ComputedStyle, output: &mut Vec<InlineAtom>) {
        let Some((kind, value)) = input_control_data(node) else {
            return;
        };
        let is_textarea = kind == ControlKind::TextArea;
        let is_button = matches!(
            kind,
            ControlKind::Submit | ControlKind::Button | ControlKind::Reset
        );
        let default_width = if is_button {
            let label = node.attr("value").unwrap_or_else(|| "Submit".into());
            (label.chars().count() as f32 * style.font_size * 0.58 + 22.0).max(70.0)
        } else if is_textarea {
            node.attr("cols")
                .and_then(|columns| columns.parse::<f32>().ok())
                .map(|columns| columns * style.font_size * 0.55 + 16.0)
                .unwrap_or(180.0)
        } else {
            node.attr("size")
                .and_then(|size| size.parse::<f32>().ok())
                .map(|size| size * style.font_size * 0.55 + 16.0)
                .unwrap_or(180.0)
        };
        let content_width =
            element_length(node, "width", style.width, default_width, style.font_size);
        let content_height = element_length(
            node,
            "height",
            style.height,
            if is_button {
                30.0
            } else if is_textarea {
                node.attr("rows")
                    .and_then(|rows| rows.parse::<f32>().ok())
                    .unwrap_or(2.0)
                    * style.line_height
                    + 10.0
            } else {
                style.line_height + 10.0
            },
            style.font_size,
        );
        let margin = style.margin.resolve(self.viewport.width, style.font_size);
        let padding = style.padding.resolve(self.viewport.width, style.font_size);
        let border = style
            .border_width
            .resolve(self.viewport.width, style.font_size);
        let horizontal_insets = padding.horizontal() + border.horizontal();
        let vertical_insets = padding.vertical() + border.vertical();
        let width = if style.box_sizing == BoxSizing::BorderBox {
            content_width
        } else {
            content_width + horizontal_insets
        };
        let height = if style.box_sizing == BoxSizing::BorderBox {
            content_height
        } else {
            content_height + vertical_insets
        };
        let icon = self.control_background_icon(style, width, height);
        let mut label = input_control_label(node, kind, &value);
        if icon.is_some() && value.is_empty() {
            label.clear();
        }
        output.push(InlineAtom::Control {
            spec: Box::new(ControlSpec {
                node_id: node_id(node),
                rect: RectF::default(),
                kind,
                name: node.attr("name").unwrap_or_default(),
                label,
                value,
                options: Vec::new(),
                selected_index: 0,
                placeholder: node
                    .attr("placeholder")
                    .or_else(|| node.attr("title"))
                    .unwrap_or_default(),
                form_id: nearest_form(node).map(|form| node_id(&form)),
                background_color: self.effective_background_color(node),
                text_color: style.color,
                border_color: style
                    .border_color
                    .composite_over(self.effective_background_color(node)),
                border_width: [border.top, border.right, border.bottom, border.left],
                border_radius: resolve_border_radius(
                    style.border_radius,
                    RectF {
                        x: 0.0,
                        y: 0.0,
                        width,
                        height,
                    },
                    style.font_size,
                ),
                padding: [padding.top, padding.right, padding.bottom, padding.left],
                font: FontSpec::from_style(style),
                icon_url: icon.as_ref().map(|(url, _, _)| url.clone()),
                icon_width: icon.as_ref().map(|(_, width, _)| *width).unwrap_or(0.0),
                icon_height: icon.as_ref().map(|(_, _, height)| *height).unwrap_or(0.0),
            }),
            width: width + margin.horizontal(),
            height: height + margin.vertical(),
            inset_x: margin.left,
            inset_y: margin.top,
            control_width: width,
            control_height: height,
        });
    }

    fn collect_select(&self, node: &NodeRef, style: &ComputedStyle, output: &mut Vec<InlineAtom>) {
        let options = Node::descendants(node)
            .skip(1)
            .filter(|descendant| descendant.tag_name() == Some("option"))
            .map(|option| {
                let label = option.text_content().trim().to_string();
                let value = option.attr("value").unwrap_or_else(|| label.clone());
                let selected = option.attr("selected").is_some();
                (SelectOption { value, label }, selected)
            })
            .collect::<Vec<_>>();
        let selected_index = options
            .iter()
            .position(|(_, selected)| *selected)
            .unwrap_or(0)
            .min(options.len().saturating_sub(1));
        let options = options
            .into_iter()
            .map(|(option, _)| option)
            .collect::<Vec<_>>();
        let selected = options.get(selected_index);
        let value = selected
            .map(|option| option.value.clone())
            .unwrap_or_default();
        let label = selected
            .map(|option| option.label.clone())
            .unwrap_or_default();
        let default_width =
            (label.chars().count() as f32 * style.font_size * 0.58 + 38.0).max(90.0);
        let content_width =
            element_length(node, "width", style.width, default_width, style.font_size);
        let content_height = element_length(
            node,
            "height",
            style.height,
            style.line_height + 10.0,
            style.font_size,
        );
        let margin = style.margin.resolve(self.viewport.width, style.font_size);
        let padding = style.padding.resolve(self.viewport.width, style.font_size);
        let border = style
            .border_width
            .resolve(self.viewport.width, style.font_size);
        let horizontal_insets = padding.horizontal() + border.horizontal();
        let vertical_insets = padding.vertical() + border.vertical();
        let width = if style.box_sizing == BoxSizing::BorderBox {
            content_width
        } else {
            content_width + horizontal_insets
        };
        let height = if style.box_sizing == BoxSizing::BorderBox {
            content_height
        } else {
            content_height + vertical_insets
        };
        output.push(InlineAtom::Control {
            spec: Box::new(ControlSpec {
                node_id: node_id(node),
                rect: RectF::default(),
                kind: ControlKind::Select,
                name: node.attr("name").unwrap_or_default(),
                value,
                label,
                options,
                selected_index,
                placeholder: String::new(),
                form_id: nearest_form(node).map(|form| node_id(&form)),
                background_color: self.effective_background_color(node),
                text_color: style.color,
                border_color: style
                    .border_color
                    .composite_over(self.effective_background_color(node)),
                border_width: [border.top, border.right, border.bottom, border.left],
                border_radius: resolve_border_radius(
                    style.border_radius,
                    RectF {
                        x: 0.0,
                        y: 0.0,
                        width,
                        height,
                    },
                    style.font_size,
                ),
                padding: [padding.top, padding.right, padding.bottom, padding.left],
                font: FontSpec::from_style(style),
                icon_url: None,
                icon_width: 0.0,
                icon_height: 0.0,
            }),
            width: width + margin.horizontal(),
            height: height + margin.vertical(),
            inset_x: margin.left,
            inset_y: margin.top,
            control_width: width,
            control_height: height,
        });
    }

    fn collect_button(&self, node: &NodeRef, style: &ComputedStyle, output: &mut Vec<InlineAtom>) {
        let label = node.text_content().trim().to_string();
        let mut icon = Node::descendants(node)
            .skip(1)
            .find(|descendant| descendant.tag_name() == Some("svg"))
            .and_then(|svg| {
                let key = inline_svg_key(&svg);
                let image = self.page.images.get(&key)?;
                let icon_style = self.styles.get(&svg);
                Some((
                    key,
                    element_length(
                        &svg,
                        "width",
                        icon_style.width,
                        image.width as f32,
                        icon_style.font_size,
                    )
                    .max(1.0),
                    element_length(
                        &svg,
                        "height",
                        icon_style.height,
                        image.height as f32,
                        icon_style.font_size,
                    )
                    .max(1.0),
                ))
            });
        let content_width = style
            .width
            .resolve(self.viewport.width, style.font_size)
            .unwrap_or_else(|| {
                if label.is_empty() {
                    icon.as_ref().map(|(_, width, _)| *width).unwrap_or(70.0)
                } else {
                    (label.chars().count() as f32 * style.font_size * 0.58 + 22.0).max(70.0)
                }
            });
        let content_height = resolve_height_value(style.height, self.viewport, style.font_size)
            .unwrap_or_else(|| {
                icon.as_ref()
                    .map(|(_, _, height)| *height)
                    .unwrap_or(style.line_height + 10.0)
                    .max(style.line_height)
            });
        let margin = style.margin.resolve(self.viewport.width, style.font_size);
        let padding = style.padding.resolve(self.viewport.width, style.font_size);
        let border = style
            .border_width
            .resolve(self.viewport.width, style.font_size);
        let width = if style.box_sizing == BoxSizing::BorderBox {
            content_width
        } else {
            content_width + padding.horizontal() + border.horizontal()
        };
        let height = if style.box_sizing == BoxSizing::BorderBox {
            content_height
        } else {
            content_height + padding.vertical() + border.vertical()
        };
        if icon.is_none() {
            icon = self.control_background_icon(style, width, height);
        }
        output.push(InlineAtom::Control {
            spec: Box::new(ControlSpec {
                node_id: node_id(node),
                rect: RectF::default(),
                kind: match node.attr("type").as_deref() {
                    Some("button") => ControlKind::Button,
                    Some("reset") => ControlKind::Reset,
                    _ => ControlKind::Submit,
                },
                name: node.attr("name").unwrap_or_default(),
                value: node.attr("value").unwrap_or_else(|| label.clone()),
                label: label.clone(),
                options: Vec::new(),
                selected_index: 0,
                placeholder: String::new(),
                form_id: nearest_form(node).map(|form| node_id(&form)),
                background_color: self.effective_background_color(node),
                text_color: style.color,
                border_color: style
                    .border_color
                    .composite_over(self.effective_background_color(node)),
                border_width: [border.top, border.right, border.bottom, border.left],
                border_radius: resolve_border_radius(
                    style.border_radius,
                    RectF {
                        x: 0.0,
                        y: 0.0,
                        width,
                        height,
                    },
                    style.font_size,
                ),
                padding: [padding.top, padding.right, padding.bottom, padding.left],
                font: FontSpec::from_style(style),
                icon_url: icon.as_ref().map(|(url, _, _)| url.clone()),
                icon_width: icon.as_ref().map(|(_, width, _)| *width).unwrap_or(0.0),
                icon_height: icon.as_ref().map(|(_, _, height)| *height).unwrap_or(0.0),
            }),
            width: width + margin.horizontal(),
            height: height + margin.vertical(),
            inset_x: margin.left,
            inset_y: margin.top,
            control_width: width,
            control_height: height,
        });
    }

    fn effective_background_color(&self, node: &NodeRef) -> Color {
        let mut colors = Vec::new();
        let mut candidate = Some(node.clone());
        while let Some(current) = candidate {
            let color = self.styles.get(&current).background_color;
            if color.alpha > 0 {
                colors.push(color);
            }
            candidate = current.parent();
        }
        colors
            .into_iter()
            .rev()
            .fold(Color::WHITE, |backdrop, color| {
                color.composite_over(backdrop)
            })
    }

    fn background_tile_rect(&self, style: &ComputedStyle, clip_rect: RectF) -> Option<RectF> {
        let url = style.background_image.as_ref()?;
        let image = self.page.images.get(url)?;
        let (width, height) = resolve_background_size(
            style.background_size,
            clip_rect,
            image.width as f32,
            image.height as f32,
            style.font_size,
            self.viewport,
        )?;
        let x = clip_rect.x
            + resolve_background_position(
                style.background_position_x,
                clip_rect.width,
                width,
                style.font_size,
                self.viewport,
            );
        let y = clip_rect.y
            + resolve_background_position(
                style.background_position_y,
                clip_rect.height,
                height,
                style.font_size,
                self.viewport,
            );
        Some(RectF {
            x,
            y,
            width,
            height,
        })
    }

    fn control_background_icon(
        &self,
        style: &ComputedStyle,
        width: f32,
        height: f32,
    ) -> Option<(String, f32, f32)> {
        if style.background_repeat_x || style.background_repeat_y {
            return None;
        }
        let tile = self.background_tile_rect(
            style,
            RectF {
                x: 0.0,
                y: 0.0,
                width,
                height,
            },
        )?;
        Some((style.background_image.clone()?, tile.width, tile.height))
    }

    fn layout_inline_atoms(
        &mut self,
        atoms: &[InlineAtom],
        x: f32,
        mut y: f32,
        width: f32,
        align: TextAlign,
        default_line_height: f32,
    ) -> f32 {
        self.begin_inline_measurement_context();
        let mut line = Vec::new();
        let mut line_width = 0.0_f32;
        let mut line_height = 0.0_f32;

        for atom in atoms {
            if matches!(atom, InlineAtom::Break) {
                y = self.paint_line(
                    &line,
                    x,
                    y,
                    width,
                    align,
                    line_width,
                    line_height.max(default_line_height),
                );
                line.clear();
                line_width = 0.0;
                line_height = 0.0;
                continue;
            }
            let measured = self.measure_atom(atom, line.is_empty());
            let should_wrap = !line.is_empty()
                && line_width + measured.width > width
                && measured.break_before
                && !measured.no_wrap;
            if should_wrap {
                y = self.paint_line(
                    &line,
                    x,
                    y,
                    width,
                    align,
                    line_width,
                    line_height.max(default_line_height),
                );
                line.clear();
                line_width = 0.0;
                line_height = 0.0;
            }
            let measured = if should_wrap {
                self.measure_atom(atom, true)
            } else {
                measured
            };
            line_width += measured.width;
            line_height = line_height.max(measured.height);
            line.push(measured);
        }
        if !line.is_empty() {
            y = self.paint_line(
                &line,
                x,
                y,
                width,
                align,
                line_width,
                line_height.max(default_line_height),
            );
        }
        y
    }

    fn begin_inline_measurement_context(&mut self) {
        // Inline atoms are short-lived per formatting context, so pointer-keyed measurements
        // must not outlive a context and alias recycled allocations from a later atom tree.
        self.measurement_cache.clear();
        self.inline_box_cache.clear();
    }

    fn measure_atom<'a>(&mut self, atom: &'a InlineAtom, line_start: bool) -> MeasuredAtom<'a> {
        let cache_key = (atom as *const InlineAtom as usize, line_start);
        if let Some(measured) = self.measurement_cache.get(&cache_key) {
            return measured.for_atom(atom);
        }
        let measured = match atom {
            InlineAtom::Text {
                text,
                font,
                line_height,
                no_wrap,
                ..
            } => {
                let break_before = text.chars().next().is_some_and(char::is_whitespace);
                let text = if line_start {
                    text.trim_start()
                } else {
                    text.as_str()
                };
                let (width, measured_height) = self.measurer.measure(text, font);
                MeasuredAtom {
                    atom,
                    text: Some(text),
                    width,
                    height: line_height.max(measured_height),
                    no_wrap: *no_wrap,
                    break_before,
                }
            }
            InlineAtom::Image { width, height, .. }
            | InlineAtom::Control { width, height, .. }
            | InlineAtom::Placeholder { width, height } => MeasuredAtom {
                atom,
                text: None,
                width: *width,
                height: *height,
                no_wrap: false,
                break_before: false,
            },
            InlineAtom::InlineBox { children, style } => {
                let metrics = self.measure_inline_box(atom, children, style);
                MeasuredAtom {
                    atom,
                    text: None,
                    width: metrics.total_width(),
                    height: metrics.total_height(),
                    no_wrap: style.white_space == WhiteSpace::NoWrap,
                    break_before: false,
                }
            }
            InlineAtom::Break => unreachable!(),
        };
        self.measurement_cache
            .insert(cache_key, CachedAtomMeasurement::from(&measured));
        measured
    }

    fn measure_inline_box(
        &mut self,
        atom: &InlineAtom,
        children: &[InlineAtom],
        style: &ComputedStyle,
    ) -> InlineBoxMetrics {
        let cache_key = atom as *const InlineAtom as usize;
        if let Some(metrics) = self.inline_box_cache.get(&cache_key) {
            return *metrics;
        }
        let mut children_width = 0.0_f32;
        let mut children_height = 0.0_f32;
        for (index, child) in children.iter().enumerate() {
            if matches!(child, InlineAtom::Break) {
                continue;
            }
            let measured = self.measure_atom(child, index == 0);
            children_width += measured.width;
            children_height = children_height.max(measured.height);
        }

        let margin = style.margin.resolve(self.viewport.width, style.font_size);
        let border = style
            .border_width
            .resolve(self.viewport.width, style.font_size);
        let padding = style.padding.resolve(self.viewport.width, style.font_size);
        let horizontal_insets = border.horizontal() + padding.horizontal();
        let vertical_insets = border.vertical() + padding.vertical();
        let mut border_box_width = resolve_outer_size(
            style.width,
            self.viewport.width,
            style.font_size,
            horizontal_insets,
            style.box_sizing,
        )
        .unwrap_or(children_width + horizontal_insets);
        if let Some(minimum) = resolve_outer_size(
            style.min_width,
            self.viewport.width,
            style.font_size,
            horizontal_insets,
            style.box_sizing,
        ) {
            border_box_width = border_box_width.max(minimum);
        }
        if let Some(maximum) = resolve_outer_size(
            style.max_width,
            self.viewport.width,
            style.font_size,
            horizontal_insets,
            style.box_sizing,
        ) {
            border_box_width = border_box_width.min(maximum);
        }

        let mut border_box_height = resolve_content_height(
            style.height,
            self.viewport,
            style.font_size,
            vertical_insets,
            style.box_sizing,
        )
        .map(|height| height + vertical_insets)
        .unwrap_or(children_height + vertical_insets);
        if let Some(minimum) = resolve_content_height(
            style.min_height,
            self.viewport,
            style.font_size,
            vertical_insets,
            style.box_sizing,
        ) {
            border_box_height = border_box_height.max(minimum + vertical_insets);
        }
        if let Some(maximum) = resolve_content_height(
            style.max_height,
            self.viewport,
            style.font_size,
            vertical_insets,
            style.box_sizing,
        ) {
            border_box_height = border_box_height.min(maximum + vertical_insets);
        }

        let metrics = InlineBoxMetrics {
            margin,
            border,
            padding,
            border_box_width: border_box_width.max(0.0),
            border_box_height: border_box_height.max(0.0),
            children_width,
        };
        self.inline_box_cache.insert(cache_key, metrics);
        metrics
    }

    #[allow(clippy::too_many_arguments)]
    fn paint_line(
        &mut self,
        line: &[MeasuredAtom<'_>],
        x: f32,
        y: f32,
        width: f32,
        align: TextAlign,
        line_width: f32,
        line_height: f32,
    ) -> f32 {
        let mut cursor_x = match align {
            TextAlign::Start => x,
            TextAlign::Center => x + ((width - line_width) / 2.0).max(0.0),
            TextAlign::End => x + (width - line_width).max(0.0),
        };
        for measured in line {
            self.paint_atom(measured, cursor_x, y, line_height);
            cursor_x += measured.width;
        }
        y + line_height
    }

    fn paint_atom(&mut self, measured: &MeasuredAtom<'_>, x: f32, y: f32, line_height: f32) {
        let atom_y = y + (line_height - measured.height).max(0.0) / 2.0;
        match measured.atom {
            InlineAtom::Text {
                font, color, link, ..
            } => {
                let text = measured.text.unwrap_or_default();
                if !text.is_empty() {
                    self.output.items.push(DisplayItem::Text {
                        rect: RectF {
                            x,
                            y: atom_y,
                            width: measured.width,
                            height: measured.height,
                        },
                        text: text.to_string(),
                        font: font.clone(),
                        color: *color,
                        link: link.clone(),
                    });
                }
            }
            InlineAtom::Image {
                url,
                alt,
                inset_x,
                inset_y,
                image_width,
                image_height,
                tint,
                ..
            } => self.output.items.push(DisplayItem::Image {
                rect: RectF {
                    x: x + inset_x,
                    y: atom_y + inset_y,
                    width: *image_width,
                    height: *image_height,
                },
                url: url.clone(),
                alt: alt.clone(),
                tint: *tint,
            }),
            InlineAtom::Control {
                spec,
                inset_x,
                inset_y,
                control_width,
                control_height,
                ..
            } => {
                let mut spec = spec.as_ref().clone();
                spec.rect = RectF {
                    x: x + inset_x,
                    y: atom_y + inset_y,
                    width: *control_width,
                    height: *control_height,
                };
                if spec.background_color.alpha > 0 {
                    self.output.items.push(DisplayItem::SolidRect {
                        rect: spec.rect,
                        color: spec.background_color,
                        radius: spec.border_radius,
                    });
                }
                if spec.border_color.alpha > 0 && spec.border_width.iter().any(|width| *width > 0.0)
                {
                    self.output.items.push(DisplayItem::BorderRect {
                        rect: spec.rect,
                        widths: spec.border_width,
                        color: spec.border_color,
                        radius: spec.border_radius,
                    });
                }
                if let Some(url) = spec.icon_url.as_ref() {
                    self.output.items.push(DisplayItem::Image {
                        rect: RectF {
                            x: spec.rect.x + (spec.rect.width - spec.icon_width).max(0.0) / 2.0,
                            y: spec.rect.y + (spec.rect.height - spec.icon_height).max(0.0) / 2.0,
                            width: spec.icon_width.min(spec.rect.width).max(0.0),
                            height: spec.icon_height.min(spec.rect.height).max(0.0),
                        },
                        url: url.clone(),
                        alt: String::new(),
                        tint: None,
                    });
                }
                self.output.items.push(DisplayItem::Control(Box::new(spec)));
            }
            InlineAtom::InlineBox { children, style } => {
                let metrics = self.measure_inline_box(measured.atom, children, style);
                let border_x = x + metrics.margin.left;
                let border_y = atom_y + metrics.margin.top;
                let border_rect = RectF {
                    x: border_x,
                    y: border_y,
                    width: metrics.border_box_width,
                    height: metrics.border_box_height,
                };
                let radius =
                    resolve_border_radius(style.border_radius, border_rect, style.font_size);
                if style.background_color.alpha > 0 {
                    self.output.items.push(DisplayItem::SolidRect {
                        rect: border_rect,
                        color: style
                            .background_color
                            .composite_over(self.output.background),
                        radius,
                    });
                }
                if let Some(tile_rect) = self.background_tile_rect(style, border_rect)
                    && let Some(url) = style.background_image.as_ref()
                {
                    self.output.items.push(DisplayItem::BackgroundImage {
                        clip_rect: border_rect,
                        tile_rect,
                        url: url.clone(),
                        repeat_x: style.background_repeat_x,
                        repeat_y: style.background_repeat_y,
                    });
                }
                if style.border_color.alpha > 0
                    && (metrics.border.horizontal() > 0.0 || metrics.border.vertical() > 0.0)
                {
                    self.output.items.push(DisplayItem::BorderRect {
                        rect: border_rect,
                        widths: [
                            metrics.border.top,
                            metrics.border.right,
                            metrics.border.bottom,
                            metrics.border.left,
                        ],
                        color: style.border_color.composite_over(
                            style
                                .background_color
                                .composite_over(self.output.background),
                        ),
                        radius,
                    });
                }
                let content_x = border_x + metrics.border.left + metrics.padding.left;
                let content_y = border_y + metrics.border.top + metrics.padding.top;
                let content_width = (metrics.border_box_width
                    - metrics.border.horizontal()
                    - metrics.padding.horizontal())
                .max(0.0);
                let content_height = (metrics.border_box_height
                    - metrics.border.vertical()
                    - metrics.padding.vertical())
                .max(0.0);
                let mut child_x = match style.text_align {
                    TextAlign::Start => content_x,
                    TextAlign::Center => {
                        content_x + ((content_width - metrics.children_width) / 2.0).max(0.0)
                    }
                    TextAlign::End => content_x + (content_width - metrics.children_width).max(0.0),
                };
                for (index, child) in children.iter().enumerate() {
                    if matches!(child, InlineAtom::Break) {
                        continue;
                    }
                    let child = self.measure_atom(child, index == 0);
                    self.paint_atom(&child, child_x, content_y, content_height.max(child.height));
                    child_x += child.width;
                }
            }
            InlineAtom::Placeholder { .. } | InlineAtom::Break => {}
        }
    }
}

#[derive(Debug)]
enum InlineAtom {
    Text {
        text: String,
        font: FontSpec,
        color: Color,
        link: Option<String>,
        line_height: f32,
        no_wrap: bool,
    },
    Image {
        url: String,
        alt: String,
        tint: Option<Color>,
        width: f32,
        height: f32,
        inset_x: f32,
        inset_y: f32,
        image_width: f32,
        image_height: f32,
    },
    Control {
        spec: Box<ControlSpec>,
        width: f32,
        height: f32,
        inset_x: f32,
        inset_y: f32,
        control_width: f32,
        control_height: f32,
    },
    InlineBox {
        children: Vec<InlineAtom>,
        style: Box<ComputedStyle>,
    },
    Placeholder {
        width: f32,
        height: f32,
    },
    Break,
}

struct MeasuredAtom<'a> {
    atom: &'a InlineAtom,
    text: Option<&'a str>,
    width: f32,
    height: f32,
    no_wrap: bool,
    break_before: bool,
}

#[derive(Debug, Clone, Copy)]
struct CachedAtomMeasurement {
    text_start: Option<usize>,
    width: f32,
    height: f32,
    no_wrap: bool,
    break_before: bool,
}

impl CachedAtomMeasurement {
    fn for_atom<'a>(&self, atom: &'a InlineAtom) -> MeasuredAtom<'a> {
        let text = match (atom, self.text_start) {
            (InlineAtom::Text { text, .. }, Some(start)) => text.get(start..),
            _ => None,
        };
        MeasuredAtom {
            atom,
            text,
            width: self.width,
            height: self.height,
            no_wrap: self.no_wrap,
            break_before: self.break_before,
        }
    }
}

impl From<&MeasuredAtom<'_>> for CachedAtomMeasurement {
    fn from(measured: &MeasuredAtom<'_>) -> Self {
        Self {
            text_start: measured.text.and_then(|measured_text| {
                let InlineAtom::Text { text, .. } = measured.atom else {
                    return None;
                };
                Some(text.len() - measured_text.len())
            }),
            width: measured.width,
            height: measured.height,
            no_wrap: measured.no_wrap,
            break_before: measured.break_before,
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct InlineBoxMetrics {
    margin: ResolvedEdges,
    border: ResolvedEdges,
    padding: ResolvedEdges,
    border_box_width: f32,
    border_box_height: f32,
    children_width: f32,
}

#[derive(Debug, Clone)]
enum GridTrack {
    Auto,
    Fixed(Length),
    Fraction(f32),
    MinMax(Box<GridTrack>, Box<GridTrack>),
}

struct GridItemPlacement {
    node: NodeRef,
    column: usize,
    column_end: usize,
    row: usize,
    row_end: usize,
}

#[derive(Clone)]
struct FlexItem {
    node: NodeRef,
    basis: f32,
    grow: f32,
    shrink: f32,
    margin_start_auto: bool,
    margin_end_auto: bool,
}

fn translate_display_items(items: &mut [DisplayItem], offset_x: f32, offset_y: f32) {
    for item in items {
        let rect = match item {
            DisplayItem::SolidRect { rect, .. }
            | DisplayItem::BorderRect { rect, .. }
            | DisplayItem::Text { rect, .. }
            | DisplayItem::Image { rect, .. } => rect,
            DisplayItem::BackgroundImage {
                clip_rect,
                tile_rect,
                ..
            } => {
                clip_rect.x += offset_x;
                clip_rect.y += offset_y;
                tile_rect.x += offset_x;
                tile_rect.y += offset_y;
                continue;
            }
            DisplayItem::Control(spec) => &mut spec.rect,
        };
        rect.x += offset_x;
        rect.y += offset_y;
    }
}

fn parse_grid_tracks(input: &str) -> Vec<GridTrack> {
    let mut tracks = Vec::new();
    for token in grid_track_tokens(input) {
        if let Some(arguments) = token
            .strip_prefix("repeat(")
            .and_then(|value| value.strip_suffix(')'))
            && let Some((count, repeated)) = split_grid_once(arguments, ',')
        {
            let repetitions = count.trim().parse::<usize>().unwrap_or(1).clamp(1, 64);
            let repeated_tracks = parse_grid_tracks(repeated);
            for _ in 0..repetitions {
                tracks.extend(repeated_tracks.iter().cloned());
            }
        } else if let Some(track) = parse_grid_track(token) {
            tracks.push(track);
        }
    }
    tracks
}

fn grid_track_tokens(input: &str) -> Vec<&str> {
    let mut tokens = Vec::new();
    let mut cursor = 0;
    let bytes = input.as_bytes();
    while cursor < bytes.len() {
        while cursor < bytes.len() && bytes[cursor].is_ascii_whitespace() {
            cursor += 1;
        }
        if cursor >= bytes.len() {
            break;
        }
        if bytes[cursor] == b'['
            && let Some(end) = input[cursor + 1..].find(']')
        {
            cursor += end + 2;
            continue;
        }

        let start = cursor;
        let mut depth = 0_i32;
        while cursor < bytes.len() {
            match bytes[cursor] {
                b'(' => depth += 1,
                b')' => {
                    depth = (depth - 1).max(0);
                    cursor += 1;
                    if depth == 0 {
                        break;
                    }
                    continue;
                }
                byte if byte.is_ascii_whitespace() && depth == 0 => break,
                _ => {}
            }
            cursor += 1;
        }
        if start < cursor {
            tokens.push(input[start..cursor].trim());
        }
    }
    tokens
}

fn parse_grid_track(token: &str) -> Option<GridTrack> {
    let token = token.trim();
    if token.is_empty() || token == "none" || token.starts_with('[') {
        return None;
    }
    if matches!(token, "auto" | "min-content" | "max-content") {
        return Some(GridTrack::Auto);
    }
    if let Some(fraction) = token.strip_suffix("fr") {
        return Some(GridTrack::Fraction(
            fraction.trim().parse::<f32>().unwrap_or(1.0).max(0.0),
        ));
    }
    if let Some(arguments) = token
        .strip_prefix("minmax(")
        .and_then(|value| value.strip_suffix(')'))
        && let Some((minimum, maximum)) = split_grid_once(arguments, ',')
    {
        return Some(GridTrack::MinMax(
            Box::new(parse_grid_track(minimum).unwrap_or(GridTrack::Auto)),
            Box::new(parse_grid_track(maximum).unwrap_or(GridTrack::Auto)),
        ));
    }
    if let Some(argument) = token
        .strip_prefix("fit-content(")
        .and_then(|value| value.strip_suffix(')'))
    {
        return parse_length(argument).map(GridTrack::Fixed);
    }
    parse_length(token).map(GridTrack::Fixed)
}

fn split_grid_once(input: &str, delimiter: char) -> Option<(&str, &str)> {
    let mut depth = 0_i32;
    for (index, character) in input.char_indices() {
        match character {
            '(' => depth += 1,
            ')' => depth = (depth - 1).max(0),
            candidate if candidate == delimiter && depth == 0 => {
                return Some((&input[..index], &input[index + character.len_utf8()..]));
            }
            _ => {}
        }
    }
    None
}

fn resolve_background_size(
    size: BackgroundSize,
    area: RectF,
    natural_width: f32,
    natural_height: f32,
    font_size: f32,
    viewport: RectF,
) -> Option<(f32, f32)> {
    if natural_width <= 0.0 || natural_height <= 0.0 {
        return None;
    }
    let (width, height) = match size {
        BackgroundSize::Auto => (natural_width, natural_height),
        BackgroundSize::Contain | BackgroundSize::Cover => {
            let horizontal = area.width / natural_width;
            let vertical = area.height / natural_height;
            let scale = if size == BackgroundSize::Contain {
                horizontal.min(vertical)
            } else {
                horizontal.max(vertical)
            };
            (natural_width * scale, natural_height * scale)
        }
        BackgroundSize::Explicit { width, height } => {
            let width = resolve_background_length(width, area.width, font_size, viewport);
            let height = resolve_background_length(height, area.height, font_size, viewport);
            match (width, height) {
                (Some(width), Some(height)) => (width, height),
                (Some(width), None) => (width, width * natural_height / natural_width),
                (None, Some(height)) => (height * natural_width / natural_height, height),
                (None, None) => (natural_width, natural_height),
            }
        }
    };
    (width > 0.0 && height > 0.0).then_some((width, height))
}

fn resolve_background_position(
    position: Length,
    area_size: f32,
    image_size: f32,
    font_size: f32,
    viewport: RectF,
) -> f32 {
    match position {
        Length::Percent(percent) => (area_size - image_size) * percent / 100.0,
        Length::Calc {
            px,
            percent,
            em,
            vw,
            vh,
        } => {
            px + (area_size - image_size) * percent / 100.0
                + font_size * em
                + viewport.width * vw / 100.0
                + viewport.height * vh / 100.0
        }
        _ => resolve_background_length(position, area_size, font_size, viewport).unwrap_or(0.0),
    }
}

fn resolve_background_length(
    length: Length,
    basis: f32,
    font_size: f32,
    viewport: RectF,
) -> Option<f32> {
    match length {
        Length::Auto => None,
        Length::Px(value) => Some(value),
        Length::Percent(value) => Some(basis * value / 100.0),
        Length::Em(value) => Some(font_size * value),
        Length::Vw(value) => Some(viewport.width * value / 100.0),
        Length::Vh(value) => Some(viewport.height * value / 100.0),
        Length::Calc {
            px,
            percent,
            em,
            vw,
            vh,
        } => Some(
            px + basis * percent / 100.0
                + font_size * em
                + viewport.width * vw / 100.0
                + viewport.height * vh / 100.0,
        ),
    }
}

fn resolve_grid_columns(
    tracks: &[GridTrack],
    available_width: f32,
    gap: f32,
    font_size: f32,
) -> Vec<f32> {
    let gap_width = gap * tracks.len().saturating_sub(1) as f32;
    let available_tracks = (available_width - gap_width).max(0.0);
    let mut sizes = Vec::with_capacity(tracks.len());
    let mut flex_factors = Vec::with_capacity(tracks.len());
    let mut automatic = Vec::with_capacity(tracks.len());
    for track in tracks {
        let (base, flex, is_auto) = grid_track_metrics(track, available_tracks, font_size);
        sizes.push(base);
        flex_factors.push(flex);
        automatic.push(is_auto);
    }

    let remaining = (available_tracks - sizes.iter().sum::<f32>()).max(0.0);
    let total_flex = flex_factors.iter().sum::<f32>();
    if total_flex > 0.0 {
        for (size, flex) in sizes.iter_mut().zip(flex_factors) {
            *size += remaining * flex / total_flex;
        }
    } else {
        let automatic_count = automatic.iter().filter(|is_auto| **is_auto).count();
        if automatic_count > 0 {
            let share = remaining / automatic_count as f32;
            for (size, is_auto) in sizes.iter_mut().zip(automatic) {
                if is_auto {
                    *size += share;
                }
            }
        }
    }
    sizes
}

fn grid_track_metrics(track: &GridTrack, basis: f32, font_size: f32) -> (f32, f32, bool) {
    match track {
        GridTrack::Auto => (0.0, 0.0, true),
        GridTrack::Fixed(length) => (
            length.resolve(basis, font_size).unwrap_or(0.0).max(0.0),
            0.0,
            false,
        ),
        GridTrack::Fraction(fraction) => (0.0, *fraction, false),
        GridTrack::MinMax(minimum, maximum) => {
            let (minimum, _, _) = grid_track_metrics(minimum, basis, font_size);
            match maximum.as_ref() {
                GridTrack::Fraction(fraction) => (minimum, *fraction, false),
                GridTrack::Fixed(length) => (
                    minimum.max(length.resolve(basis, font_size).unwrap_or(minimum)),
                    0.0,
                    false,
                ),
                GridTrack::Auto => (minimum, 0.0, true),
                GridTrack::MinMax(_, _) => (minimum, 0.0, true),
            }
        }
    }
}

fn resolve_grid_row_minimum(track: &GridTrack, basis: f32, font_size: f32) -> f32 {
    match track {
        GridTrack::Auto | GridTrack::Fraction(_) => 0.0,
        GridTrack::Fixed(length) => length.resolve(basis, font_size).unwrap_or(0.0).max(0.0),
        GridTrack::MinMax(minimum, maximum) => match maximum.as_ref() {
            GridTrack::Fixed(length) => length.resolve(basis, font_size).unwrap_or(0.0).max(0.0),
            _ => resolve_grid_row_minimum(minimum, basis, font_size),
        },
    }
}

impl InlineBoxMetrics {
    fn total_width(self) -> f32 {
        self.margin.horizontal() + self.border_box_width
    }

    fn total_height(self) -> f32 {
        self.margin.vertical() + self.border_box_height
    }
}

fn collect_text_atoms(
    text: &str,
    style: &ComputedStyle,
    link: Option<String>,
    output: &mut Vec<InlineAtom>,
    pending_space: &mut bool,
) {
    if style.white_space == WhiteSpace::Pre {
        for (index, line) in text.replace("\r\n", "\n").split('\n').enumerate() {
            if index > 0 {
                output.push(InlineAtom::Break);
            }
            if !line.is_empty() {
                output.push(text_atom(line.to_string(), style, link.clone()));
            }
        }
        return;
    }

    let mut word_start = None;
    let mut saw_space = *pending_space;
    for (index, character) in text.char_indices() {
        if character.is_whitespace() {
            if let Some(start) = word_start.take() {
                let mut word = text[start..index].to_string();
                if saw_space {
                    word.insert(0, ' ');
                }
                output.push(text_atom(word, style, link.clone()));
            }
            saw_space = true;
        } else if word_start.is_none() {
            word_start = Some(index);
        }
    }
    if let Some(start) = word_start {
        let mut word = text[start..].to_string();
        if saw_space {
            word.insert(0, ' ');
        }
        output.push(text_atom(word, style, link));
        *pending_space = false;
    } else {
        *pending_space = saw_space;
    }
    if text.chars().last().is_some_and(char::is_whitespace) {
        *pending_space = true;
    }
}

fn text_atom(text: String, style: &ComputedStyle, link: Option<String>) -> InlineAtom {
    InlineAtom::Text {
        text,
        font: FontSpec::from_style(style),
        color: style.color,
        link,
        line_height: style.line_height,
        no_wrap: style.white_space == WhiteSpace::NoWrap,
    }
}

fn is_block_level(display: Display) -> bool {
    matches!(
        display,
        Display::Block
            | Display::Flex
            | Display::Grid
            | Display::Table
            | Display::TableRow
            | Display::TableCell
    )
}

fn resolve_border_radius(radius: Length, rect: RectF, font_size: f32) -> f32 {
    let basis = rect.width.min(rect.height).max(0.0);
    radius
        .resolve(basis, font_size)
        .unwrap_or(0.0)
        .clamp(0.0, basis / 2.0)
}

fn node_id(node: &NodeRef) -> NodeId {
    node.id()
}

fn svg_uses_current_color(node: &NodeRef) -> bool {
    Node::descendants(node).any(|descendant| {
        ["fill", "stroke", "style"].iter().any(|attribute| {
            descendant
                .attr(attribute)
                .is_some_and(|value| value.to_ascii_lowercase().contains("currentcolor"))
        })
    })
}

fn resolve_outer_size(
    length: Length,
    basis: f32,
    font_size: f32,
    insets: f32,
    box_sizing: BoxSizing,
) -> Option<f32> {
    length
        .resolve(basis, font_size)
        .map(|size| match box_sizing {
            BoxSizing::ContentBox => size + insets,
            BoxSizing::BorderBox => size,
        })
}

fn resolve_content_height(
    length: Length,
    viewport: RectF,
    font_size: f32,
    insets: f32,
    box_sizing: BoxSizing,
) -> Option<f32> {
    resolve_height_value(length, viewport, font_size).map(|size| match box_sizing {
        BoxSizing::ContentBox => size,
        BoxSizing::BorderBox => (size - insets).max(0.0),
    })
}

fn resolve_height_value(length: Length, viewport: RectF, font_size: f32) -> Option<f32> {
    match length {
        Length::Auto | Length::Percent(_) => None,
        Length::Px(value) => Some(value),
        Length::Em(value) => Some(value * font_size),
        Length::Vh(value) => Some(viewport.height * value / 100.0),
        Length::Vw(value) => Some(viewport.width * value / 100.0),
        Length::Calc {
            px,
            percent,
            em,
            vw,
            vh,
        } if percent.abs() <= f32::EPSILON => {
            Some(px + font_size * em + viewport.width * vw / 100.0 + viewport.height * vh / 100.0)
        }
        Length::Calc { .. } => None,
    }
}

fn style_collapses_overflow(style: &ComputedStyle, viewport: RectF) -> bool {
    style.overflow_hidden
        && resolve_height_value(style.max_height, viewport, style.font_size)
            .is_some_and(|height| height <= 0.0)
}

fn element_length(
    node: &NodeRef,
    attribute: &str,
    css: Length,
    fallback: f32,
    font_size: f32,
) -> f32 {
    css.resolve(fallback, font_size)
        .or_else(|| {
            node.attr(attribute)
                .and_then(|value| value.trim_end_matches("px").parse::<f32>().ok())
        })
        .unwrap_or(fallback)
        .max(0.0)
}

fn input_control_data(node: &NodeRef) -> Option<(ControlKind, String)> {
    let tag = node.tag_name()?;
    if tag == "textarea" {
        return Some((ControlKind::TextArea, node.text_content()));
    }
    if tag != "input" {
        return None;
    }
    let input_type = node
        .attr("type")
        .unwrap_or_else(|| "text".into())
        .to_ascii_lowercase();
    if matches!(
        input_type.as_str(),
        "hidden" | "checkbox" | "radio" | "file"
    ) {
        return None;
    }
    let kind = match input_type.as_str() {
        "password" => ControlKind::Password,
        "search" => ControlKind::Search,
        "submit" => ControlKind::Submit,
        "button" => ControlKind::Button,
        "reset" => ControlKind::Reset,
        _ => ControlKind::Text,
    };
    Some((kind, node.attr("value").unwrap_or_default()))
}

fn input_control_label(node: &NodeRef, kind: ControlKind, value: &str) -> String {
    if !matches!(
        kind,
        ControlKind::Submit | ControlKind::Button | ControlKind::Reset
    ) || !value.is_empty()
    {
        return value.to_string();
    }
    let label = node
        .attr("aria-label")
        .or_else(|| node.attr("title"))
        .or_else(|| node.attr("alt"))
        .unwrap_or_default();
    if kind == ControlKind::Submit && label.eq_ignore_ascii_case("search") {
        "Go".to_string()
    } else {
        label
    }
}

fn default_control_content_height(
    node: &NodeRef,
    kind: &ControlKind,
    style: &ComputedStyle,
) -> f32 {
    match kind {
        ControlKind::Submit | ControlKind::Button | ControlKind::Reset => 30.0,
        ControlKind::TextArea => {
            node.attr("rows")
                .and_then(|rows| rows.parse::<f32>().ok())
                .unwrap_or(2.0)
                * style.line_height
                + 10.0
        }
        _ => style.line_height + 10.0,
    }
}

fn nearest_form(node: &NodeRef) -> Option<NodeRef> {
    if let Some(form_id) = node.attr("form") {
        let mut root = node.clone();
        while let Some(parent) = root.parent() {
            root = parent;
        }
        if let Some(form) = Node::descendants(&root).find(|candidate| {
            candidate.tag_name() == Some("form")
                && candidate.attr("id").as_deref() == Some(form_id.as_str())
        }) {
            return Some(form);
        }
    }
    let mut ancestor = node.parent();
    while let Some(candidate) = ancestor {
        if candidate.tag_name() == Some("form") {
            return Some(candidate);
        }
        ancestor = candidate.parent();
    }
    None
}

fn collect_forms(page: &Page) -> HashMap<NodeId, FormSpec> {
    page.dom
        .elements_named("form")
        .map(|form| {
            let node_id = node_id(&form);
            let action = form
                .attr("action")
                .and_then(|action| resolve_url(&page.source_url, &action))
                .unwrap_or_else(|| page.source_url.clone());
            let method = form
                .attr("method")
                .unwrap_or_else(|| "get".into())
                .to_ascii_lowercase();
            let hidden_fields = super::dom::Node::descendants(&page.dom.document)
                .filter(|node| node.tag_name() == Some("input"))
                .filter(|node| nearest_form(node).is_some_and(|owner| owner.id() == form.id()))
                .filter(|node| {
                    node.attr("type")
                        .is_some_and(|kind| kind.eq_ignore_ascii_case("hidden"))
                })
                .filter_map(|node| {
                    Some((node.attr("name")?, node.attr("value").unwrap_or_default()))
                })
                .collect();
            (
                node_id,
                FormSpec {
                    node_id,
                    action,
                    method,
                    hidden_fields,
                },
            )
        })
        .collect()
}

fn table_rows(node: &NodeRef) -> Vec<NodeRef> {
    let mut rows = Vec::new();
    let mut stack = node
        .children
        .borrow()
        .iter()
        .rev()
        .cloned()
        .collect::<Vec<_>>();
    while let Some(candidate) = stack.pop() {
        if candidate.tag_name() == Some("tr") {
            rows.push(candidate);
        } else if matches!(candidate.tag_name(), Some("thead" | "tbody" | "tfoot")) {
            stack.extend(candidate.children.borrow().iter().rev().cloned());
        }
    }
    rows
}

fn table_cell_widths(cells: &[NodeRef], width: f32, styles: &StyleSet) -> Vec<f32> {
    let mut widths = vec![None; cells.len()];
    let mut assigned = 0.0;
    for (index, cell) in cells.iter().enumerate() {
        let length = cell
            .attr("width")
            .and_then(|value| {
                if let Some(percent) = value.strip_suffix('%') {
                    percent.parse::<f32>().ok().map(Length::Percent)
                } else {
                    value.parse::<f32>().ok().map(Length::Px)
                }
            })
            .or_else(|| (styles.get(cell).width != Length::Auto).then_some(styles.get(cell).width));
        if let Some(resolved) =
            length.and_then(|length| length.resolve(width, styles.get(cell).font_size))
        {
            widths[index] = Some(resolved);
            assigned += resolved;
        }
    }
    let auto_count = widths.iter().filter(|width| width.is_none()).count().max(1);
    let automatic = ((width - assigned).max(0.0) / auto_count as f32).max(1.0);
    widths
        .into_iter()
        .map(|value| value.unwrap_or(automatic))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    struct FixedMeasurer;

    impl TextMeasurer for FixedMeasurer {
        fn measure(&mut self, text: &str, font: &FontSpec) -> (f32, f32) {
            (text.chars().count() as f32 * font.size * 0.5, font.size)
        }
    }

    #[derive(Default)]
    struct CountingMeasurer {
        calls: usize,
    }

    impl TextMeasurer for CountingMeasurer {
        fn measure(&mut self, text: &str, font: &FontSpec) -> (f32, f32) {
            self.calls += 1;
            (text.chars().count() as f32 * font.size * 0.5, font.size)
        }
    }

    #[test]
    fn lays_out_centered_image_form_and_links() {
        let mut page = Page::parse(
            r#"
                <style>body{margin:0} center{text-align:center}.logo{padding:20px 0}
                .search{width:300px;height:24px} a{color:#123456}</style>
                <center><img class="logo" src="/logo.png" width="100" height="40"><br>
                <form action="/search"><input class="search" name="q"><br>
                <input type="submit" value="Search"></form><a href="/about">About</a></center>
            "#,
            "https://example.com/",
        );
        page.images.insert(
            "https://example.com/logo.png".into(),
            super::super::page::DecodedImage {
                width: 100,
                height: 40,
                bgra: vec![0; 100 * 40 * 4],
            },
        );
        let mut measurer = FixedMeasurer;
        let output = layout_page(&page, 800.0, 600.0, &mut measurer);
        let logo = output
            .items
            .iter()
            .find_map(|item| match item {
                DisplayItem::Image { rect, .. } => Some(*rect),
                _ => None,
            })
            .unwrap();
        assert!((logo.x - 350.0).abs() < 1.0);
        let controls = output
            .items
            .iter()
            .filter(|item| matches!(item, DisplayItem::Control(_)))
            .count();
        assert_eq!(controls, 2);
        assert!(output.items.iter().any(|item| matches!(item, DisplayItem::Text { link: Some(link), .. } if link == "https://example.com/about")));
    }

    #[test]
    fn associates_external_controls_and_hidden_fields_with_their_form_owner() {
        let page = Page::parse(
            r#"<form id="search" action="/find"></form>
               <input form="search" name="q" value="rust">
               <input form="search" type="hidden" name="lang" value="en">"#,
            "https://example.com/",
        );
        let form = page.dom.elements_named("form").next().unwrap();
        let form_id = node_id(&form);
        let mut measurer = FixedMeasurer;
        let output = layout_page(&page, 800.0, 600.0, &mut measurer);

        assert!(output.items.iter().any(|item| {
            matches!(item, DisplayItem::Control(control) if control.name == "q" && control.form_id == Some(form_id))
        }));
        assert_eq!(
            output.forms[&form_id].hidden_fields,
            [("lang".into(), "en".into())]
        );
    }

    #[test]
    fn evaluates_media_queries_against_the_style_viewport() {
        let page = Page::parse(
            r#"<style>@media (min-width: 1100px) { p { color: #c00 } }</style><p>Wide</p>"#,
            "https://example.com/",
        );
        let mut measurer = FixedMeasurer;
        let output = layout_page_with_style_viewport(&page, 1080.0, 600.0, 1110.0, &mut measurer);

        assert!(output.items.iter().any(|item| {
            matches!(item, DisplayItem::Text { text, color, .. }
                if text == "Wide" && *color == Color::rgb(204, 0, 0))
        }));
    }

    #[test]
    fn centers_explicitly_sized_background_images_in_block_boxes() {
        let mut page = Page::parse(
            r#"<style>
                body { margin: 0 }
                .logo {
                    display: block;
                    width: 65px;
                    height: 60px;
                    background: no-repeat center/auto 36px url('/logo.svg');
                }
               </style><a class="logo"></a>"#,
            "https://example.com/",
        );
        page.images.insert(
            "https://example.com/logo.svg".into(),
            super::super::page::DecodedImage {
                width: 48,
                height: 48,
                bgra: vec![0; 48 * 48 * 4],
            },
        );
        let mut measurer = FixedMeasurer;
        let output = layout_page(&page, 800.0, 600.0, &mut measurer);
        let (clip, tile, repeat_x, repeat_y) = output
            .items
            .iter()
            .find_map(|item| match item {
                DisplayItem::BackgroundImage {
                    clip_rect,
                    tile_rect,
                    repeat_x,
                    repeat_y,
                    ..
                } => Some((*clip_rect, *tile_rect, *repeat_x, *repeat_y)),
                _ => None,
            })
            .unwrap();
        assert_eq!(clip.width, 65.0);
        assert_eq!(clip.height, 60.0);
        assert!((tile.x - 14.5).abs() < 0.01);
        assert!((tile.y - 12.0).abs() < 0.01);
        assert_eq!(tile.width, 36.0);
        assert_eq!(tile.height, 36.0);
        assert!(!repeat_x);
        assert!(!repeat_y);
    }

    #[test]
    fn preserves_spaces_between_inline_elements() {
        let page = Page::parse(
            "<p>Hello <span>wide</span> world</p>",
            "https://example.com/",
        );
        let mut measurer = FixedMeasurer;
        let output = layout_page(&page, 800.0, 600.0, &mut measurer);
        let text = output
            .items
            .iter()
            .filter_map(|item| match item {
                DisplayItem::Text { text, .. } => Some(text.as_str()),
                _ => None,
            })
            .collect::<String>();
        assert_eq!(text, "Hello wide world");
    }

    #[test]
    fn caches_measurements_for_nested_inline_boxes() {
        let page = Page::parse(
            r#"<p><span style="background: red"><span style="background: blue">cached measurement</span></span></p>"#,
            "https://example.com/",
        );
        let mut measurer = CountingMeasurer::default();
        let output = layout_page(&page, 800.0, 600.0, &mut measurer);
        let text = output
            .items
            .iter()
            .filter_map(|item| match item {
                DisplayItem::Text { text, .. } => Some(text.as_str()),
                _ => None,
            })
            .collect::<String>();

        assert_eq!(text, "cached measurement");
        assert_eq!(measurer.calls, 2);
    }

    #[test]
    fn skips_intrinsic_measurement_for_a_definite_flex_basis() {
        let definite_page = Page::parse(
            r#"<div style="display:flex"><span style="width:100px">definite basis</span></div>"#,
            "https://example.com/",
        );
        let automatic_page = Page::parse(
            r#"<div style="display:flex"><span>automatic basis</span></div>"#,
            "https://example.com/",
        );
        let mut definite_measurer = CountingMeasurer::default();
        layout_page(&definite_page, 800.0, 600.0, &mut definite_measurer);
        let mut automatic_measurer = CountingMeasurer::default();
        layout_page(&automatic_page, 800.0, 600.0, &mut automatic_measurer);

        assert!(definite_measurer.calls < automatic_measurer.calls);
    }

    #[test]
    fn vertical_margins_do_not_make_normal_inline_text_unbreakable() {
        let page = Page::parse(
            r#"<style>
                body { margin: 0 }
                .column { width: 100px }
                a { margin: 0 0 .2em }
               </style><div class="column"><a href="/result">alpha beta gamma delta</a></div>"#,
            "https://example.com/",
        );
        let mut measurer = FixedMeasurer;
        let output = layout_page(&page, 800.0, 600.0, &mut measurer);
        let lines = output
            .items
            .iter()
            .filter_map(|item| match item {
                DisplayItem::Text {
                    rect,
                    link: Some(link),
                    ..
                } if link == "https://example.com/result" => Some(rect.y),
                _ => None,
            })
            .fold(Vec::<f32>::new(), |mut lines, y| {
                if !lines.iter().any(|line| (line - y).abs() < 0.01) {
                    lines.push(y);
                }
                lines
            });
        assert!(
            lines.len() >= 2,
            "expected wrapped inline text, got {lines:?}"
        );
    }

    #[test]
    fn does_not_break_before_punctuation_at_inline_boundaries() {
        let page = Page::parse(
            r#"<style>body { margin: 0 } p { width: 84px; margin: 0 }</style>
               <p>alpha <b>beta</b>. gamma</p>"#,
            "https://example.com/",
        );
        let mut measurer = FixedMeasurer;
        let output = layout_page(&page, 800.0, 600.0, &mut measurer);
        let text_items = output
            .items
            .iter()
            .filter_map(|item| match item {
                DisplayItem::Text { rect, text, .. } => Some((text.as_str(), rect.y)),
                _ => None,
            })
            .collect::<Vec<_>>();
        let beta_y = text_items
            .iter()
            .find_map(|(text, y)| text.contains("beta").then_some(*y))
            .unwrap();
        let punctuation_y = text_items
            .iter()
            .find_map(|(text, y)| (*text == ".").then_some(*y))
            .unwrap();
        let gamma_y = text_items
            .iter()
            .find_map(|(text, y)| text.contains("gamma").then_some(*y))
            .unwrap();
        assert_eq!(punctuation_y, beta_y);
        assert!(gamma_y > punctuation_y);
    }

    #[test]
    fn places_explicit_grid_items_across_fractional_and_fixed_tracks() {
        let page = Page::parse(
            r#"
                <style>
                    body { margin: 0 }
                    #container { display: flex }
                    .grid { display: grid; width: 900px;
                            grid-template-columns: 1fr 1fr 300px }
                    .main { grid-area: 1 / 1 / 2 / 3; height: 40px; background: #ff0000 }
                    .side { grid-area: 1 / 3 / 2 / 4; height: 60px; background: #0000ff }
                </style>
                <div id="container"><div class="grid">
                    <main class="main"></main><aside class="side"></aside>
                </div></div>
            "#,
            "https://example.com/",
        );
        let mut measurer = FixedMeasurer;
        let output = layout_page(&page, 900.0, 600.0, &mut measurer);
        let main = output
            .items
            .iter()
            .find_map(|item| match item {
                DisplayItem::SolidRect { rect, color, .. } if *color == Color::rgb(255, 0, 0) => {
                    Some(*rect)
                }
                _ => None,
            })
            .unwrap();
        let side = output
            .items
            .iter()
            .find_map(|item| match item {
                DisplayItem::SolidRect { rect, color, .. } if *color == Color::rgb(0, 0, 255) => {
                    Some(*rect)
                }
                _ => None,
            })
            .unwrap();
        assert_eq!(
            main,
            RectF {
                x: 0.0,
                y: 0.0,
                width: 600.0,
                height: 40.0
            }
        );
        assert_eq!(
            side,
            RectF {
                x: 600.0,
                y: 0.0,
                width: 300.0,
                height: 60.0
            }
        );
    }

    #[test]
    fn resolves_percentage_radius_against_the_finished_box() {
        let page = Page::parse(
            r#"<style>body{margin:0}.pill{width:100px;height:40px;background:red;border-radius:50%}</style>
               <div class="pill"></div>"#,
            "https://example.com/",
        );
        let mut measurer = FixedMeasurer;
        let output = layout_page(&page, 300.0, 200.0, &mut measurer);
        let radius = output
            .items
            .iter()
            .find_map(|item| match item {
                DisplayItem::SolidRect { radius, .. } => Some(*radius),
                _ => None,
            })
            .unwrap();
        assert_eq!(radius, 20.0);
    }

    #[test]
    fn centers_flex_items_with_automatic_inline_margins() {
        let page = Page::parse(
            r#"<style>
                body { margin: 0 }
                .row { display: flex; width: 300px }
                .item { width: 100px; height: 20px; margin: 0 auto; background: red }
               </style><div class="row"><div class="item"></div></div>"#,
            "https://example.com/",
        );
        let mut measurer = FixedMeasurer;
        let output = layout_page(&page, 300.0, 200.0, &mut measurer);
        let item = output
            .items
            .iter()
            .find_map(|item| match item {
                DisplayItem::SolidRect { rect, color, .. } if *color == Color::rgb(255, 0, 0) => {
                    Some(*rect)
                }
                _ => None,
            })
            .unwrap();
        assert_eq!(item.x, 100.0);
    }

    #[test]
    fn treats_indefinite_percentage_heights_as_auto_and_hides_zero_max_height_overflow() {
        let page = Page::parse(
            r#"<style>
                body { margin: 0 }
                .column { display: flex; flex-direction: column; width: 200px }
                .indefinite { height: 100%; background: red }
                .collapsed { max-height: 0; overflow: hidden }
                .after { height: 20px; background: blue }
               </style><div class="column">
                 <div class="indefinite"></div>
                 <div class="collapsed">must not paint</div>
                 <div class="after"></div>
               </div>"#,
            "https://example.com/",
        );
        let mut measurer = FixedMeasurer;
        let output = layout_page(&page, 300.0, 200.0, &mut measurer);
        let after = output
            .items
            .iter()
            .find_map(|item| match item {
                DisplayItem::SolidRect { rect, color, .. } if *color == Color::rgb(0, 0, 255) => {
                    Some(*rect)
                }
                _ => None,
            })
            .unwrap();
        assert_eq!(after.y, 0.0);
        assert!(!output.items.iter().any(
            |item| matches!(item, DisplayItem::Text { text, .. } if text.contains("must not paint"))
        ));
    }

    #[test]
    fn preserves_textarea_semantics_for_native_controls() {
        let page = Page::parse(
            r#"<form action="/search"><textarea name="q" rows="1">hello</textarea></form>"#,
            "https://example.com/",
        );
        let mut measurer = FixedMeasurer;
        let output = layout_page(&page, 800.0, 600.0, &mut measurer);
        let control = output
            .items
            .iter()
            .find_map(|item| match item {
                DisplayItem::Control(control) => Some(control),
                _ => None,
            })
            .unwrap();
        assert_eq!(control.kind, ControlKind::TextArea);
        assert_eq!(control.name, "q");
        assert_eq!(control.value, "hello");
    }

    #[test]
    fn preserves_block_level_replaced_form_controls() {
        let page = Page::parse(
            r#"<style>body{margin:0}input{display:block;width:100%;height:44px;border:0}</style>
               <form action="/search"><input name="q" value="test"></form>"#,
            "https://example.com/",
        );
        let mut measurer = FixedMeasurer;
        let output = layout_page(&page, 300.0, 200.0, &mut measurer);
        let control = output
            .items
            .iter()
            .find_map(|item| match item {
                DisplayItem::Control(control) => Some(control),
                _ => None,
            })
            .unwrap();
        assert_eq!(control.kind, ControlKind::Text);
        assert_eq!(control.name, "q");
        assert_eq!(control.value, "test");
        assert_eq!(control.rect.width, 300.0);
        assert_eq!(control.rect.height, 44.0);
    }

    #[test]
    fn represents_select_as_one_native_control_instead_of_all_option_text() {
        let page = Page::parse(
            r#"<style>body{margin:0}</style><form action="/search">
               <select name="region"><option value="all">All Regions</option>
               <option value="ca" selected>Canada</option></select></form>"#,
            "https://example.com/",
        );
        let mut measurer = FixedMeasurer;
        let output = layout_page(&page, 300.0, 200.0, &mut measurer);
        let control = output
            .items
            .iter()
            .find_map(|item| match item {
                DisplayItem::Control(control) => Some(control),
                _ => None,
            })
            .unwrap();
        assert_eq!(control.kind, ControlKind::Select);
        assert_eq!(control.name, "region");
        assert_eq!(control.value, "ca");
        assert_eq!(control.label, "Canada");
        assert_eq!(control.options.len(), 2);
        assert!(!output.items.iter().any(
            |item| matches!(item, DisplayItem::Text { text, .. } if text.contains("All RegionsCanada"))
        ));
    }

    #[test]
    fn keeps_transparent_borders_in_layout_without_painting_them() {
        let page = Page::parse(
            r#"<style>body{margin:0}.result{height:20px;border:1px solid rgba(0,0,0,0)}</style>
               <div class="result"></div>"#,
            "https://example.com/",
        );
        let mut measurer = FixedMeasurer;
        let output = layout_page(&page, 300.0, 200.0, &mut measurer);
        assert!(
            !output
                .items
                .iter()
                .any(|item| matches!(item, DisplayItem::BorderRect { .. }))
        );
    }

    #[test]
    fn renders_noscript_fallback_when_script_execution_is_unavailable() {
        let page = Page::parse(
            r#"
                <script>script-only text</script>
                <noscript>
                    <style>div { display:none }</style>
                    <div style="display:block">Script-free fallback</div>
                </noscript>
            "#,
            "https://example.com/",
        );
        let mut measurer = FixedMeasurer;
        let output = layout_page(&page, 800.0, 600.0, &mut measurer);
        let text = output
            .items
            .iter()
            .filter_map(|item| match item {
                DisplayItem::Text { text, .. } => Some(text.as_str()),
                _ => None,
            })
            .collect::<String>();
        assert_eq!(text, "Script-free fallback");
    }
}
