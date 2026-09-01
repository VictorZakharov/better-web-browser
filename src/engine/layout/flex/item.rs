//! Layout of one flex item's resolved main size.

use super::super::*;

impl<M: TextMeasurer> LayoutEngine<'_, M> {
    pub(super) fn layout_flex_item(
        &mut self,
        item: &FlexItem,
        x: f32,
        y: f32,
        width: f32,
        percentage_basis: f32,
        containing_height: Option<f32>,
        container_style: &ComputedStyle,
    ) -> BlockMetrics {
        let Some(node) = item.node.as_ref() else {
            let bottom = self.layout_inline_atoms(
                &item.anonymous_atoms,
                x,
                y,
                width,
                container_style.text_align,
                container_style.line_height,
            );
            return BlockMetrics { bottom };
        };
        let original_style = self.styles.get(node).clone();
        let stretched_sizes = containing_height
            .filter(|_| {
                container_style.align_items == AlignItems::Stretch
                    && original_style.height == Length::Auto
                    && original_style.margin.top != Length::Auto
                    && original_style.margin.bottom != Length::Auto
            })
            .map(|cross_size| {
                let margins = original_style
                    .margin
                    .resolve(percentage_basis, original_style.font_size);
                let border = original_style
                    .border_width
                    .resolve(percentage_basis, original_style.font_size);
                let padding = original_style
                    .padding
                    .resolve(percentage_basis, original_style.font_size);
                let border_box = (cross_size - margins.vertical()).max(0.0);
                (
                    border_box,
                    (border_box - border.vertical() - padding.vertical()).max(0.0),
                )
            });
        let tag = node.tag_name().unwrap_or_default();
        if !matches!(
            tag,
            "img" | "image" | "input" | "textarea" | "button" | "svg"
        ) {
            return self.layout_block_with_content_height(
                node,
                x,
                y,
                width,
                containing_height,
                Some(UsedInlineSize {
                    outer: width,
                    percentage_basis,
                }),
                stretched_sizes.map(|(_, content)| content),
            );
        }

        let mut style = original_style;
        let margin = style.margin.resolve(percentage_basis, style.font_size);
        let border = style
            .border_width
            .resolve(percentage_basis, style.font_size);
        let padding = style.padding.resolve(percentage_basis, style.font_size);
        let border_box_width = (width - margin.horizontal()).max(1.0);
        style.width = Length::Px(if style.box_sizing == BoxSizing::BorderBox {
            border_box_width
        } else {
            (border_box_width - border.horizontal() - padding.horizontal()).max(1.0)
        });
        if let Some((border_box, content)) = stretched_sizes {
            style.height = Length::Px(if style.box_sizing == BoxSizing::BorderBox {
                border_box
            } else {
                content
            });
        }

        let mut atoms = Vec::new();
        match tag {
            "img" | "image" => self.collect_image(
                node,
                &style,
                None,
                &mut atoms,
                InlineContainingBlock {
                    width,
                    height: containing_height,
                },
            ),
            "input" | "textarea" => self.collect_input(
                node,
                &style,
                &mut atoms,
                InlineContainingBlock {
                    width,
                    height: containing_height,
                },
            ),
            "button" => self.collect_button(
                node,
                &style,
                &mut atoms,
                InlineContainingBlock {
                    width,
                    height: containing_height,
                },
            ),
            "svg" => self.collect_svg(
                node,
                &style,
                &mut atoms,
                InlineContainingBlock {
                    width,
                    height: containing_height,
                },
            ),
            _ => {}
        }
        let bottom =
            self.layout_inline_atoms(&atoms, x, y, width, style.text_align, style.line_height);
        BlockMetrics { bottom }
    }
}
