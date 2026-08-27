use super::*;

impl<M: TextMeasurer> LayoutEngine<'_, M> {
    pub(super) fn collect_input(
        &self,
        node: &NodeRef,
        style: &ComputedStyle,
        output: &mut Vec<InlineAtom>,
    ) {
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

    pub(super) fn collect_select(
        &self,
        node: &NodeRef,
        style: &ComputedStyle,
        output: &mut Vec<InlineAtom>,
    ) {
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

    pub(super) fn collect_button(
        &self,
        node: &NodeRef,
        style: &ComputedStyle,
        output: &mut Vec<InlineAtom>,
    ) {
        let label = self.visible_control_label(node);
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
            })
            .or_else(|| self.control_mask_icon(node));
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

    fn visible_control_label(&self, node: &NodeRef) -> String {
        fn append<M: TextMeasurer>(
            engine: &LayoutEngine<'_, M>,
            node: &NodeRef,
            text: &mut String,
        ) {
            let style = engine.styles.get(node);
            if style.display == Display::None
                || !style.visibility
                || style_collapses_overflow(style, engine.viewport)
            {
                return;
            }
            if let NodeData::Text(value) = &node.data {
                text.push_str(&value.borrow());
                return;
            }
            for child in Node::composed_children(node).iter() {
                append(engine, child, text);
            }
        }

        let mut label = String::new();
        append(self, node, &mut label);
        label.trim().to_string()
    }

    fn control_mask_icon(&self, node: &NodeRef) -> Option<(String, f32, f32)> {
        Node::composed_descendants(node)
            .skip(1)
            .find_map(|descendant| {
                let style = self.styles.get(&descendant);
                let url = style.mask_image.as_ref()?;
                let image = self.page.images.get(url)?;
                Some((
                    url.clone(),
                    element_length(
                        &descendant,
                        "width",
                        style.width,
                        image.width as f32,
                        style.font_size,
                    ),
                    element_length(
                        &descendant,
                        "height",
                        style.height,
                        image.height as f32,
                        style.font_size,
                    ),
                ))
            })
    }
}
