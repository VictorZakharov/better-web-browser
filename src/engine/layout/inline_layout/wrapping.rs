//! Fit complete unbreakable runs, including runs crossing styled inline nodes.
use super::*;

impl<M: TextMeasurer> LayoutEngine<'_, M> {
    pub(super) fn unbreakable_run_widths(&mut self, atoms: &[InlineAtom], basis: f32) -> Vec<f32> {
        let mut widths = vec![0.0; atoms.len()];
        let mut following = 0.0;
        for (index, atom) in atoms.iter().enumerate().rev() {
            if matches!(atom, InlineAtom::Break) {
                following = 0.0;
                continue;
            }
            let measured = self.measure_atom(atom, false, basis);
            widths[index] = measured.width + following
                - if following == 0.0 {
                    self.hanging_space_width(atom)
                } else {
                    0.0
                };
            following = if self.inline_break_before(atoms, index) {
                0.0
            } else {
                widths[index]
            };
        }
        widths
    }

    pub(in crate::engine::layout) fn hanging_space_width(&mut self, atom: &InlineAtom) -> f32 {
        // CSS Text §4.1.2: pre-wrap end-of-line spaces hang, so they do not force
        // an otherwise fitting word onto another line or shift text alignment.
        let InlineAtom::Text {
            text,
            font,
            preserve_space: true,
            no_wrap: false,
            ..
        } = atom
        else {
            return 0.0;
        };
        let trimmed = text.trim_end_matches([' ', '\t']);
        if trimmed.len() == text.len() {
            return 0.0;
        }
        self.measurer.measure(text, font).0 - self.measurer.measure(trimmed, font).0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::layout::test_support::FixedMeasurer;

    #[test]
    fn atomic_edges_wrap_on_both_sides_but_obey_word_joiner() {
        for (markup, expected_y) in [
            ("<span></span>after", 20.0),
            ("before<span></span>", 20.0),
            ("<span></span>&#x2060;after", 0.0),
            ("before&#x2060;<span></span>", 0.0),
        ] {
            let page = Page::parse(
                &format!(
                    "<style>body{{margin:0}}main{{width:70px;line-height:20px}}span{{display:inline-block;width:50px;height:20px}}</style><main>{markup}</main>"
                ),
                "https://example.test/",
            );
            let output = layout_page(&page, 800.0, 600.0, &mut FixedMeasurer);
            let span = page.dom.elements_named("span").next().unwrap();
            if markup.starts_with("before") {
                assert_eq!(output.node_bounds[&span.id()].y, expected_y, "{markup}");
            } else {
                let y = output
                    .items
                    .iter()
                    .find_map(|item| match item {
                        DisplayItem::Text { text, rect, .. } if text.contains("after") => {
                            Some(rect.y)
                        }
                        _ => None,
                    })
                    .unwrap();
                assert_eq!(y, expected_y + 2.0, "{markup}");
            }
        }
    }

    #[test]
    fn inline_block_list_shrinks_to_fit_and_wraps_between_adjacent_items() {
        let page = Page::parse(
            "<style>body{margin:0}table{width:200px;float:right}td{padding:0}ul{display:inline-block;margin:0;padding:0}li{display:inline-block;padding-right:4px}li:after{content:', '}</style><table><tr><td><ul><li>Alpha</li><li>Beta</li><li>Gamma</li><li>Delta</li><li>Epsilon</li><li>Zeta</li></ul></td></tr></table>",
            "https://example.test/",
        );
        let output = layout_page(&page, 800.0, 600.0, &mut FixedMeasurer);
        let table = page.dom.elements_named("table").next().unwrap();
        let table = output.node_bounds[&table.id()];
        assert_eq!(table.width, 200.0);
        let cells: Vec<_> = page
            .dom
            .elements_named("li")
            .map(|n| output.node_bounds[&n.id()])
            .collect();
        assert!(cells.last().unwrap().y > cells[0].y, "{cells:?}");
        assert!(
            cells
                .iter()
                .all(|cell| cell.x >= table.x && cell.right() <= table.right()),
            "{cells:?}"
        );
        let geometry =
            layout_geometry_with_style_viewport(&page, 800.0, 600.0, 800.0, &mut FixedMeasurer);
        assert_eq!(output.node_bounds, geometry.node_bounds);
    }

    #[test]
    fn inline_block_has_its_own_wrapping_and_block_flow() {
        let page = Page::parse(
            "<style>body{margin:0}main{width:120px}span{display:inline-block;max-width:100%;line-height:20px}p{margin:0}</style><main><span><p>first line words</p><p>second paragraph</p></span></main>",
            "https://example.test/",
        );
        let output = layout_page(&page, 800.0, 600.0, &mut FixedMeasurer);
        let paragraphs: Vec<_> = page
            .dom
            .elements_named("p")
            .map(|n| output.node_bounds[&n.id()])
            .collect();
        assert!(paragraphs[1].y >= paragraphs[0].bottom());
        let span = page.dom.elements_named("span").next().unwrap();
        let span = output.node_bounds[&span.id()];
        assert!(span.width <= 120.0 && span.height >= 80.0, "{span:?}");
    }

    #[test]
    fn atomic_boundary_wrapping_uses_parent_not_child_white_space() {
        for (parent, child, wraps) in [("normal", "nowrap", true), ("nowrap", "normal", false)] {
            let page = Page::parse(
                &format!(
                    "<style>body{{margin:0}}main{{width:90px;white-space:{parent}}}span{{display:inline-block;width:60px;height:20px;white-space:{child}}}</style><main><span>A</span><span>B</span></main>"
                ),
                "https://example.test/",
            );
            let output = layout_page(&page, 800.0, 600.0, &mut FixedMeasurer);
            let boxes: Vec<_> = page
                .dom
                .elements_named("span")
                .map(|n| output.node_bounds[&n.id()])
                .collect();
            assert_eq!(
                boxes[1].y > boxes[0].y,
                wraps,
                "{parent}/{child}: {boxes:?}"
            );
        }
    }

    #[test]
    fn wrapping_space_before_nowrap_span_breaks_before_the_entire_span() {
        let page = Page::parse(
            "<style>body{margin:0}div{width:120px;line-height:20px}span{white-space:nowrap}</style><div>Average rainy days <span>(≥ 1.0 mm)</span></div>",
            "https://example.test/",
        );
        let output = layout_page(&page, 400.0, 300.0, &mut FixedMeasurer);
        let mut span_lines = Vec::new();
        for item in output.items {
            if let DisplayItem::Text { rect, text, .. } = item {
                assert!(rect.right() <= 120.0, "{text}: {rect:?}");
                if matches!(text.trim(), "(≥" | "1.0" | "mm)") {
                    span_lines.push(rect.y);
                }
            }
        }
        assert_eq!(span_lines.len(), 3);
        assert!(span_lines.iter().all(|y| *y == span_lines[0]));
    }

    #[test]
    fn unbreakable_run_across_inline_styles_moves_as_a_unit() {
        let page = Page::parse(
            "<style>body{margin:0}div{width:80px;line-height:20px}</style><div>aaaa bb<b>cccc</b></div>",
            "https://example.test/",
        );
        let output = layout_page(&page, 400.0, 300.0, &mut FixedMeasurer);
        for item in output.items {
            if let DisplayItem::Text { rect, text, .. } = item {
                assert!(rect.right() <= 80.0, "{text}: {rect:?}");
                if matches!(text.trim(), "bb" | "cccc") {
                    assert!(rect.y >= 20.0);
                }
            }
        }
    }
}
