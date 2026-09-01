//! Normal-flow block children, inline runs, and floats.

use super::super::*;

impl<M: TextMeasurer> LayoutEngine<'_, M> {
    pub(in crate::engine::layout) fn layout_block_children(
        &mut self,
        node: &NodeRef,
        x: f32,
        mut y: f32,
        width: f32,
        containing_height: Option<f32>,
        style: &ComputedStyle,
    ) -> f32 {
        let mut atoms = Vec::new();
        let mut pending_space = false;
        let mut left_float_width = 0.0_f32;
        let mut right_float_width = 0.0_f32;
        let mut float_bottom = y;
        if node.tag_name() == Some("li") && style.list_style_type != ListStyleType::None {
            atoms.push(InlineAtom::Text {
                text: "• ".into(),
                font: FontSpec::from_style(style),
                color: style.color,
                link: None,
                node_id: None,
                line_height: style.line_height,
                no_wrap: false,
            });
        }
        for child in self.block_formatting_children(node).iter() {
            if y >= float_bottom {
                left_float_width = 0.0;
                right_float_width = 0.0;
                float_bottom = y;
            }
            let child_style = self.styles.get(child);
            if matches!(child_style.position, Position::Absolute | Position::Fixed) {
                // Defer until the completed padding box can resolve percentage geometry.
                continue;
            } else if is_block_level(child_style.display) && child_style.float != Float::None {
                let remaining_width = (width - left_float_width - right_float_width).max(0.0);
                let float_width = self
                    .flex_item_basis(child, child_style, remaining_width)
                    .clamp(0.0, remaining_width);
                let float_x = if child_style.float == Float::Right {
                    x + width - right_float_width - float_width
                } else {
                    x + left_float_width
                };
                let metrics = self.layout_block(
                    child,
                    float_x,
                    y,
                    float_width,
                    containing_height,
                    Some(UsedInlineSize {
                        outer: float_width,
                        percentage_basis: remaining_width,
                    }),
                );
                float_bottom = float_bottom.max(metrics.bottom);
                if child_style.float == Float::Right {
                    right_float_width += float_width;
                } else {
                    left_float_width += float_width;
                }
            } else if is_block_level(child_style.display) {
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
                if y >= float_bottom {
                    left_float_width = 0.0;
                    right_float_width = 0.0;
                }
                let child_width = (width - left_float_width - right_float_width).max(0.0);
                y = self
                    .layout_block(
                        child,
                        x + left_float_width,
                        y,
                        child_width,
                        containing_height,
                        None,
                    )
                    .bottom;
            } else {
                self.collect_inline(
                    child,
                    None,
                    &mut atoms,
                    &mut pending_space,
                    true,
                    InlineContainingBlock {
                        width: (width - left_float_width - right_float_width).max(0.0),
                        height: containing_height,
                    },
                );
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
}
