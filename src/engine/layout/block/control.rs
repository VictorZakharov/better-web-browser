use super::super::*;

impl<M: TextMeasurer> LayoutEngine<'_, M> {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn project_control(
        &mut self,
        node: &NodeRef,
        block_control: Option<(ControlKind, String)>,
        style: &ComputedStyle,
        rect: RectF,
        borders: ResolvedEdges,
        padding: ResolvedEdges,
        authored_content: bool,
    ) {
        if let Some((kind, value)) = block_control {
            let icon = self.control_background_icon(style, rect.width, rect.height);
            let mut label = input_control_label(node, kind, &value);
            if icon.is_some() && value.is_empty() {
                label.clear();
            }
            let value = if node.tag_name() == Some("button") {
                node.attr("value").unwrap_or_default()
            } else {
                value
            };
            self.output
                .items
                .push(DisplayItem::Control(Box::new(ControlSpec {
                    node_id: node_id(node),
                    authored_content,
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
                    border_radius: resolve_border_radius(
                        style.border_radius,
                        rect,
                        style.font_size,
                    ),
                    padding: [padding.top, padding.right, padding.bottom, padding.left],
                    font: FontSpec::from_style(style),
                    icon_url: icon.as_ref().map(|(url, _, _)| url.clone()),
                    icon_width: icon.as_ref().map(|(_, width, _)| *width).unwrap_or(0.0),
                    icon_height: icon.as_ref().map(|(_, _, height)| *height).unwrap_or(0.0),
                })));
        }
    }
}
