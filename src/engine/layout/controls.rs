mod button;
use super::*;

impl<M: TextMeasurer> LayoutEngine<'_, M> {
    pub(super) fn collect_input(
        &self,
        node: &NodeRef,
        style: &ComputedStyle,
        output: &mut Vec<InlineAtom>,
        containing_block: InlineContainingBlock,
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
        let content_width = resolve_replaced_length(
            node,
            "width",
            style.width,
            Some(containing_block.width),
            style.font_size,
        )
        .unwrap_or(default_width);
        let content_height = resolve_replaced_length(
            node,
            "height",
            style.height,
            containing_block.height,
            style.font_size,
        )
        .unwrap_or_else(|| default_control_content_height(node, &kind, style));
        let margin = style
            .margin
            .resolve(containing_block.width, style.font_size);
        let padding = style
            .padding
            .resolve(containing_block.width, style.font_size);
        let border = style
            .border_width
            .resolve(containing_block.width, style.font_size);
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
                authored_content: false,
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
            opacity: style.opacity,
        });
    }

    pub(super) fn collect_select(
        &self,
        node: &NodeRef,
        style: &ComputedStyle,
        output: &mut Vec<InlineAtom>,
        containing_block: InlineContainingBlock,
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
        let content_width = resolve_replaced_length(
            node,
            "width",
            style.width,
            Some(containing_block.width),
            style.font_size,
        )
        .unwrap_or(default_width);
        let content_height = resolve_replaced_length(
            node,
            "height",
            style.height,
            containing_block.height,
            style.font_size,
        )
        .unwrap_or(style.line_height + 10.0);
        let margin = style
            .margin
            .resolve(containing_block.width, style.font_size);
        let padding = style
            .padding
            .resolve(containing_block.width, style.font_size);
        let border = style
            .border_width
            .resolve(containing_block.width, style.font_size);
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
                authored_content: false,
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
            opacity: style.opacity,
        });
    }
}
