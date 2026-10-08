//! Bound author-controlled indices before allocating or slicing track vectors.
use super::*;

pub(super) fn clamp_area(start: usize, end: usize) -> (usize, usize) {
    // An area entirely beyond the UA limit becomes a single track at the edge;
    // a partly intersecting area retains its start and truncates only its span.
    // https://drafts.csswg.org/css-grid-2/#overlarge-grids
    let start = start.min(MAX_GRID_TRACKS - 1);
    (start, end.min(MAX_GRID_TRACKS).max(start + 1))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::layout::test_support::FixedMeasurer;

    #[test]
    fn whole_and_partial_overflow_have_distinct_clamping() {
        assert_eq!(clamp_area(2, 5), (2, 5));
        assert_eq!(clamp_area(2, usize::MAX), (2, MAX_GRID_TRACKS));
        assert_eq!(clamp_area(usize::MAX, usize::MAX), (9999, 10000));
        assert_eq!(clamp_area(10000, 10001), (9999, 10000));
        assert_eq!(clamp_area(0, 0), (0, 1));
    }

    #[test]
    fn enormous_row_indices_do_not_allocate_author_sized_vectors() {
        for row in ["10001", "2147483647", "18446744073709551615"] {
            let page = Page::parse(
                &format!(
                    "<style>body{{margin:0}}main{{display:grid;width:100px;row-gap:1px}}
                    div{{height:10px;grid-row-start:{row}}}</style><main><div></div></main>"
                ),
                "https://example.test/",
            );
            let output = layout_page(&page, 800.0, 600.0, &mut FixedMeasurer);
            let item = page.dom.elements_named("div").next().unwrap();
            let bounds = output.node_bounds[&item.id()];
            assert_eq!(bounds.y, 9999.0, "{row}");
            assert_eq!(bounds.height, 10.0, "{row}");
            assert!(output.content_height < 11000.0, "{row}");
        }
    }

    #[test]
    fn enormous_row_end_retains_the_in_range_start() {
        let page = Page::parse(
            "<style>body{margin:0}main{display:grid;width:100px;row-gap:1px}
            div{height:10px;grid-row:2 / 2147483647}</style><main><div></div></main>",
            "https://example.test/",
        );
        let output = layout_page(&page, 800.0, 600.0, &mut FixedMeasurer);
        let item = page.dom.elements_named("div").next().unwrap();
        assert_eq!(output.node_bounds[&item.id()].y, 1.0);
        assert!(output.content_height < 11000.0);
    }
}
