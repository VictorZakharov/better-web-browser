//! Normal block flow shares float exclusions with its enclosing formatting context.
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
            let child_style = self.styles.get(child);
            if child_style.display == Display::None
                || matches!(child_style.position, Position::Absolute | Position::Fixed)
            {
                continue;
            }
            if child_style.float != Float::None {
                // A float after inline content cannot rise above the preceding line.
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
                self.layout_float(child, x, y, width, containing_height);
            } else if is_block_level(child_style.display) {
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
                let margins = child_style.margin.resolve(width, child_style.font_size);
                y = y.max(self.floats.clearance(child_style.clear, y + margins.top) - margins.top);
                let mut child_x = x;
                let mut child_width = width;
                let mut used_width = None;
                if super::floats::establishes_context(child_style) {
                    let insets = child_style
                        .padding
                        .resolve(width, child_style.font_size)
                        .horizontal()
                        + table::resolved_table_borders(child, child_style, width).horizontal();
                    let needed = if child_style.width == Length::Auto {
                        resolve_outer_size(
                            child_style.min_width,
                            width,
                            child_style.font_size,
                            insets,
                            child_style.box_sizing,
                        )
                        .unwrap_or(0.0)
                            + margins.horizontal()
                    } else {
                        super::sizing::resolve_used_border_box_width(
                            child_style,
                            width,
                            insets,
                            margins,
                            width,
                            None,
                        ) + margins.horizontal()
                    };
                    (child_x, y, child_width) =
                        self.floats.fit(x, y, width, needed.max(0.01), 0.01);
                    if child_style.width != Length::Auto
                        || (child_style.display != Display::Table && child_width < width)
                    {
                        used_width = Some(UsedInlineSize {
                            outer: super::sizing::resolve_used_border_box_width(
                                child_style,
                                width,
                                insets,
                                margins,
                                (child_width - margins.horizontal()).max(0.0),
                                None,
                            ) + margins.horizontal(),
                            percentage_basis: width,
                        });
                    }
                }
                y = self
                    .layout_block(
                        child,
                        child_x,
                        y,
                        child_width,
                        containing_height,
                        used_width,
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
                        width,
                        height: containing_height,
                    },
                );
            }
        }
        if !atoms.is_empty() {
            y = self.layout_inline_atoms(&atoms, x, y, width, style.text_align, style.line_height);
        }
        y
    }
}
