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
            widths[index] = measured.width + following;
            following = if measured.break_before && !measured.no_wrap {
                0.0
            } else {
                widths[index]
            };
        }
        widths
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::layout::test_support::FixedMeasurer;

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
