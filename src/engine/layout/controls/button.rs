use super::*;

impl<M: TextMeasurer> LayoutEngine<'_, M> {
    pub(in crate::engine::layout) fn collect_button(
        &self,
        node: &NodeRef,
        style: &ComputedStyle,
        output: &mut Vec<InlineAtom>,
        containing_block: InlineContainingBlock,
    ) {
        let label = self.visible_control_label(node);
        let mut icon = Node::composed_descendants(node)
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
        let content_width = resolve_replaced_length(
            node,
            "width",
            style.width,
            Some(containing_block.width),
            style.font_size,
        )
        .unwrap_or_else(|| {
            if label.is_empty() {
                icon.as_ref().map(|(_, width, _)| *width).unwrap_or(70.0)
            } else {
                (label.chars().count() as f32 * style.font_size * 0.58 + 22.0).max(70.0)
            }
        });
        let content_height = resolve_replaced_length(
            node,
            "height",
            style.height,
            containing_block.height,
            style.font_size,
        )
        .unwrap_or_else(|| {
            icon.as_ref()
                .map(|(_, _, height)| *height)
                .unwrap_or(style.line_height + 10.0)
                .max(style.line_height)
        });
        let margin = style
            .margin
            .resolve(containing_block.width, style.font_size);
        let padding = style
            .padding
            .resolve(containing_block.width, style.font_size);
        let border = style
            .border_width
            .resolve(containing_block.width, style.font_size);
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
                authored_content: false,
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
            opacity: style.opacity,
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
