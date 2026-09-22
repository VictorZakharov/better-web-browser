use super::super::*;

#[cfg(test)]
mod tests;

impl<M: TextMeasurer> LayoutEngine<'_, M> {
    pub(super) fn button_fit_content_width(
        &mut self,
        node: &NodeRef,
        basis: f32,
        available: f32,
    ) -> f32 {
        // HTML button layout: auto inline-size is fit-content even for block and
        // absolutely positioned buttons. Out-of-flow labels do not contribute.
        // https://html.spec.whatwg.org/multipage/rendering.html#button-layout
        let margins = self
            .styles
            .get(node)
            .margin
            .resolve(basis, self.styles.get(node).font_size);
        let (minimum, preferred) = self.float_intrinsic_widths(node, basis);
        (preferred - margins.horizontal()).min(available.max(minimum - margins.horizontal()))
    }

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
        if self.emit_paint
            && let Some((kind, value)) = block_control
        {
            let icon = self.control_background_icon(style, rect.width, rect.height);
            let (invalid, validation_message) = control_feedback(node);
            let select = (kind == ControlKind::Select).then(|| select_data(node));
            let label = select.as_ref().map_or_else(
                || input_control_label(node, kind, &value),
                |select| select.label(),
            );
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
                    selected_index: select.as_ref().map_or(-1, |select| select.selected_index),
                    options: select.map_or_else(Vec::new, |select| select.options),
                    placeholder: node
                        .attr("placeholder")
                        .or_else(|| node.attr("title"))
                        .unwrap_or_default(),
                    form_id: nearest_form(node).map(|form| node_id(&form)),
                    background_color: self.control_background_color(node, style, kind),
                    text_color: style.color,
                    placeholder_color: self
                        .styles
                        .placeholder_color(node, self.effective_background_color(node)),
                    border_colors: style
                        .painted_border_colors(self.effective_background_color(node)),
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
                    invalid,
                    validation_message,
                })));
        }
    }
}
