use super::css::{
    BoxSizing, Color, ComputedStyle, Display, Float, Length, ResolvedEdges, StyleSet, TextAlign,
    WhiteSpace,
};
use super::dom::{NodeData, NodeRef};
use super::page::{Page, inline_svg_key};
use crate::navigation::resolve_url;
use std::collections::HashMap;
use std::rc::Rc;

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

#[derive(Debug, Clone, PartialEq)]
pub enum ControlKind {
    Text,
    TextArea,
    Password,
    Search,
    Submit,
    Button,
    Reset,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ControlSpec {
    pub node_id: usize,
    pub rect: RectF,
    pub kind: ControlKind,
    pub name: String,
    pub value: String,
    pub placeholder: String,
    pub form_id: Option<usize>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FormSpec {
    pub node_id: usize,
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
    },
    Control(ControlSpec),
}

#[derive(Debug, Default)]
pub struct LayoutOutput {
    pub items: Vec<DisplayItem>,
    pub content_height: f32,
    pub background: Color,
    pub forms: HashMap<usize, FormSpec>,
}

pub fn layout_page<M: TextMeasurer>(
    page: &Page,
    viewport_width: f32,
    viewport_height: f32,
    measurer: &mut M,
) -> LayoutOutput {
    let styles = page.style(viewport_width);
    let mut engine = LayoutEngine {
        page,
        styles: &styles,
        measurer,
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
        engine.output.background = body_style.background_color;
    }
    let metrics = engine.layout_block(&root, 0.0, 0.0, viewport_width.max(1.0));
    engine.output.content_height = metrics.bottom.max(viewport_height);
    engine.output
}

struct LayoutEngine<'a, M> {
    page: &'a Page,
    styles: &'a StyleSet,
    measurer: &'a mut M,
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
        if matches!(
            style.position,
            super::css::Position::Absolute | super::css::Position::Fixed
        ) {
            if let Some(left) = style.left.resolve(containing_width, style.font_size) {
                x = if style.position == super::css::Position::Fixed {
                    self.viewport.x + left
                } else {
                    containing_x + left
                };
            } else if let Some(right) = style.right.resolve(containing_width, style.font_size) {
                x = containing_x + containing_width - border_box_width - right;
            }
            if let Some(top) = style.top.resolve(self.viewport.height, style.font_size) {
                border_y = top;
            }
        }

        let content_x = x + borders.left + padding.left;
        let content_y = border_y + borders.top + padding.top;
        let content_width =
            (border_box_width - borders.horizontal() - padding.horizontal()).max(0.0);
        let background_index = if style.background_color.alpha > 0 {
            let index = self.output.items.len();
            self.output.items.push(DisplayItem::SolidRect {
                rect: RectF {
                    x,
                    y: border_y,
                    width: border_box_width,
                    height: 0.0,
                },
                color: style.background_color,
                radius: style.border_radius,
            });
            Some(index)
        } else {
            None
        };

        let content_bottom = match style.display {
            Display::Flex => self.layout_flex(node, content_x, content_y, content_width, &style),
            Display::Table => self.layout_table(node, content_x, content_y, content_width, &style),
            _ => self.layout_block_children(node, content_x, content_y, content_width, &style),
        };
        let natural_content_height = (content_bottom - content_y).max(0.0);
        let vertical_insets = borders.vertical() + padding.vertical();
        let specified_height = resolve_content_size(
            style.height,
            self.viewport.height,
            style.font_size,
            vertical_insets,
            style.box_sizing,
        );
        let minimum_height = resolve_content_size(
            style.min_height,
            self.viewport.height,
            style.font_size,
            vertical_insets,
            style.box_sizing,
        )
        .unwrap_or(0.0);
        let content_height = specified_height
            .unwrap_or(natural_content_height)
            .max(minimum_height);
        let border_box_height =
            borders.top + padding.top + content_height + padding.bottom + borders.bottom;
        let rect = RectF {
            x,
            y: border_y,
            width: border_box_width,
            height: border_box_height,
        };
        if let Some(index) = background_index
            && let DisplayItem::SolidRect { rect: target, .. } = &mut self.output.items[index]
        {
            *target = rect;
        }
        if borders.vertical() > 0.0 || borders.horizontal() > 0.0 {
            self.output.items.push(DisplayItem::BorderRect {
                rect,
                widths: [borders.top, borders.right, borders.bottom, borders.left],
                color: style.border_color,
                radius: style.border_radius,
            });
        }

        let flow_bottom = border_y + border_box_height + margins.bottom;
        BlockMetrics {
            bottom: if matches!(
                style.position,
                super::css::Position::Absolute | super::css::Position::Fixed
            ) {
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
        let mut atoms = Vec::new();
        let mut pending_space = false;
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
                && !matches!(
                    child_style.position,
                    super::css::Position::Absolute | super::css::Position::Fixed
                )
            {
                if !atoms.is_empty() {
                    y = self.layout_inline_atoms(
                        &atoms,
                        x,
                        y,
                        width,
                        style.text_align,
                        style.line_height,
                    );
                    atoms.clear();
                    pending_space = false;
                }
                y = self.layout_block(child, x, y, width).bottom;
            } else if is_block_level(child_style.display) {
                self.layout_block(child, x, y, width);
            } else {
                self.collect_inline(child, None, &mut atoms, &mut pending_space, true);
            }
        }
        if !atoms.is_empty() {
            y = self.layout_inline_atoms(&atoms, x, y, width, style.text_align, style.line_height);
        }
        y
    }

    fn layout_flex(
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
                    "button" => self.collect_button(node, style, output),
                    "svg" => {
                        let width =
                            element_length(node, "width", style.width, 24.0, style.font_size);
                        let height =
                            element_length(node, "height", style.height, 24.0, style.font_size);
                        let key = inline_svg_key(node);
                        if self.page.images.contains_key(&key) {
                            let margin = style.margin.resolve(self.viewport.width, style.font_size);
                            let padding =
                                style.padding.resolve(self.viewport.width, style.font_size);
                            output.push(InlineAtom::Image {
                                url: key,
                                alt: node.attr("aria-label").unwrap_or_default(),
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
                    _ => {
                        if style.display == Display::InlineBlock {
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
                                style: style.clone(),
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
        let Some(url) = node
            .attr("src")
            .or_else(|| node.attr("href"))
            .and_then(|src| resolve_url(&self.page.source_url, &src))
        else {
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
            width: width + margin.horizontal() + padding.horizontal() + border.horizontal(),
            height: height + margin.vertical() + padding.vertical() + border.vertical(),
            inset_x: margin.left + padding.left + border.left,
            inset_y: margin.top + padding.top + border.top,
            image_width: width,
            image_height: height,
        });
    }

    fn collect_input(&self, node: &NodeRef, style: &ComputedStyle, output: &mut Vec<InlineAtom>) {
        let is_textarea = node.tag_name() == Some("textarea");
        let input_type = node
            .attr("type")
            .unwrap_or_else(|| "text".into())
            .to_ascii_lowercase();
        if matches!(
            input_type.as_str(),
            "hidden" | "checkbox" | "radio" | "file"
        ) {
            return;
        }
        let kind = if is_textarea {
            ControlKind::TextArea
        } else {
            match input_type.as_str() {
                "password" => ControlKind::Password,
                "search" => ControlKind::Search,
                "submit" => ControlKind::Submit,
                "button" => ControlKind::Button,
                "reset" => ControlKind::Reset,
                _ => ControlKind::Text,
            }
        };
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
        output.push(InlineAtom::Control {
            spec: ControlSpec {
                node_id: node_id(node),
                rect: RectF::default(),
                kind,
                name: node.attr("name").unwrap_or_default(),
                value: if is_textarea {
                    node.text_content()
                } else {
                    node.attr("value").unwrap_or_default()
                },
                placeholder: node
                    .attr("placeholder")
                    .or_else(|| node.attr("title"))
                    .unwrap_or_default(),
                form_id: nearest_form(node).map(|form| node_id(&form)),
            },
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
        let width = style
            .width
            .resolve(self.viewport.width, style.font_size)
            .unwrap_or_else(|| {
                (label.chars().count() as f32 * style.font_size * 0.58 + 22.0).max(70.0)
            });
        let height = style
            .height
            .resolve(self.viewport.height, style.font_size)
            .unwrap_or(style.line_height + 10.0);
        output.push(InlineAtom::Control {
            spec: ControlSpec {
                node_id: node_id(node),
                rect: RectF::default(),
                kind: match node.attr("type").as_deref() {
                    Some("button") => ControlKind::Button,
                    Some("reset") => ControlKind::Reset,
                    _ => ControlKind::Submit,
                },
                name: node.attr("name").unwrap_or_default(),
                value: node.attr("value").unwrap_or(label),
                placeholder: String::new(),
                form_id: nearest_form(node).map(|form| node_id(&form)),
            },
            width,
            height,
            inset_x: 0.0,
            inset_y: 0.0,
            control_width: width,
            control_height: height,
        });
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
            let should_wrap =
                !line.is_empty() && line_width + measured.width > width && !measured.no_wrap;
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
            let measured = if line.is_empty() {
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

    fn measure_atom<'a>(&mut self, atom: &'a InlineAtom, line_start: bool) -> MeasuredAtom<'a> {
        match atom {
            InlineAtom::Text {
                text,
                font,
                line_height,
                no_wrap,
                ..
            } => {
                let text = if line_start {
                    text.trim_start()
                } else {
                    text.as_str()
                };
                let (width, measured_height) = self.measurer.measure(text, font);
                MeasuredAtom {
                    atom,
                    text: Some(text.to_string()),
                    width,
                    height: line_height.max(measured_height),
                    no_wrap: *no_wrap,
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
            },
            InlineAtom::InlineBox { children, style } => {
                let metrics = self.measure_inline_box(children, style);
                MeasuredAtom {
                    atom,
                    text: None,
                    width: metrics.total_width(),
                    height: metrics.total_height(),
                    no_wrap: style.white_space == WhiteSpace::NoWrap,
                }
            }
            InlineAtom::Break => unreachable!(),
        }
    }

    fn measure_inline_box(
        &mut self,
        children: &[InlineAtom],
        style: &ComputedStyle,
    ) -> InlineBoxMetrics {
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

        let mut border_box_height = resolve_outer_size(
            style.height,
            self.viewport.height,
            style.font_size,
            vertical_insets,
            style.box_sizing,
        )
        .unwrap_or(children_height + vertical_insets);
        if let Some(minimum) = resolve_outer_size(
            style.min_height,
            self.viewport.height,
            style.font_size,
            vertical_insets,
            style.box_sizing,
        ) {
            border_box_height = border_box_height.max(minimum);
        }

        InlineBoxMetrics {
            margin,
            border,
            padding,
            border_box_width: border_box_width.max(0.0),
            border_box_height: border_box_height.max(0.0),
            children_width,
        }
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
                let text = measured.text.as_deref().unwrap_or_default();
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
            }),
            InlineAtom::Control {
                spec,
                inset_x,
                inset_y,
                control_width,
                control_height,
                ..
            } => {
                let mut spec = spec.clone();
                spec.rect = RectF {
                    x: x + inset_x,
                    y: atom_y + inset_y,
                    width: *control_width,
                    height: *control_height,
                };
                self.output.items.push(DisplayItem::Control(spec));
            }
            InlineAtom::InlineBox { children, style } => {
                let metrics = self.measure_inline_box(children, style);
                let border_x = x + metrics.margin.left;
                let border_y = atom_y + metrics.margin.top;
                let border_rect = RectF {
                    x: border_x,
                    y: border_y,
                    width: metrics.border_box_width,
                    height: metrics.border_box_height,
                };
                if style.background_color.alpha > 0 {
                    self.output.items.push(DisplayItem::SolidRect {
                        rect: border_rect,
                        color: style.background_color,
                        radius: style.border_radius,
                    });
                }
                if metrics.border.horizontal() > 0.0 || metrics.border.vertical() > 0.0 {
                    self.output.items.push(DisplayItem::BorderRect {
                        rect: border_rect,
                        widths: [
                            metrics.border.top,
                            metrics.border.right,
                            metrics.border.bottom,
                            metrics.border.left,
                        ],
                        color: style.border_color,
                        radius: style.border_radius,
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
        width: f32,
        height: f32,
        inset_x: f32,
        inset_y: f32,
        image_width: f32,
        image_height: f32,
    },
    Control {
        spec: ControlSpec,
        width: f32,
        height: f32,
        inset_x: f32,
        inset_y: f32,
        control_width: f32,
        control_height: f32,
    },
    InlineBox {
        children: Vec<InlineAtom>,
        style: ComputedStyle,
    },
    Placeholder {
        width: f32,
        height: f32,
    },
    Break,
}

struct MeasuredAtom<'a> {
    atom: &'a InlineAtom,
    text: Option<String>,
    width: f32,
    height: f32,
    no_wrap: bool,
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
        Display::Block | Display::Flex | Display::Table | Display::TableRow | Display::TableCell
    )
}

fn node_id(node: &NodeRef) -> usize {
    Rc::as_ptr(node) as usize
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

fn resolve_content_size(
    length: Length,
    basis: f32,
    font_size: f32,
    insets: f32,
    box_sizing: BoxSizing,
) -> Option<f32> {
    length
        .resolve(basis, font_size)
        .map(|size| match box_sizing {
            BoxSizing::ContentBox => size,
            BoxSizing::BorderBox => (size - insets).max(0.0),
        })
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

fn nearest_form(node: &NodeRef) -> Option<NodeRef> {
    let mut ancestor = node.parent();
    while let Some(candidate) = ancestor {
        if candidate.tag_name() == Some("form") {
            return Some(candidate);
        }
        ancestor = candidate.parent();
    }
    None
}

fn collect_forms(page: &Page) -> HashMap<usize, FormSpec> {
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
            let hidden_fields = super::dom::Node::descendants(&form)
                .filter(|node| node.tag_name() == Some("input"))
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
}
