use super::*;
use crate::engine::layout::test_support::FixedMeasurer;

fn boxes(css: &str, markup: &str, ids: &[&str]) -> Vec<RectF> {
    let page = Page::parse(
        &format!("<style>body{{margin:0}}{css}</style>{markup}"),
        "https://example.test/",
    );
    let layout = layout_page(&page, 800.0, 600.0, &mut FixedMeasurer);
    ids.iter()
        .map(|id| {
            let node = Node::descendants(&page.dom.document)
                .find(|node| node.attr("id").as_deref() == Some(id))
                .unwrap();
            layout.node_bounds[&node.id()]
        })
        .collect()
}

#[test]
fn horizontal_implicit_tracks_use_percentages_of_the_grid_not_each_item() {
    let bounds = boxes(
        "main{display:grid;grid-auto-flow:column;grid-auto-columns:calc(25% - 7.5px);gap:10px;width:400px}div{aspect-ratio:1}",
        "<main><div id=a></div><div id=b></div><div id=c></div><div id=d></div><div id=e></div></main>",
        &["a", "b", "d", "e"],
    );
    assert_eq!(bounds[0].width, 92.5);
    assert_eq!(bounds[0].height, 92.5);
    assert_eq!(
        (bounds[1].x, bounds[2].x, bounds[3].x),
        (102.5, 307.5, 410.0)
    );
    assert!(bounds.iter().all(|rect| rect.y == 0.0));
}

#[test]
fn implicit_track_patterns_repeat_after_the_explicit_grid() {
    let bounds = boxes(
        "main{display:grid;grid-auto-flow:column;grid-template-columns:30px;grid-auto-columns:40px 60px;grid-template-rows:20px;width:400px}",
        "<main><div id=a></div><div id=b></div><div id=c></div><div id=d></div></main>",
        &["a", "b", "c", "d"],
    );
    assert_eq!(
        bounds.iter().map(|rect| rect.width).collect::<Vec<_>>(),
        [30.0, 40.0, 60.0, 40.0]
    );
}

#[test]
fn automatic_items_skip_explicit_occupied_areas_regardless_of_dom_order() {
    let bounds = boxes(
        "main{display:grid;grid-template-columns:40px 40px;grid-auto-rows:20px;width:80px}#fixed{grid-area:1 / 1 / 2 / 2}",
        "<main><div id=a></div><div id=fixed></div><div id=b></div></main>",
        &["a", "fixed", "b"],
    );
    assert_eq!((bounds[0].x, bounds[0].y), (40.0, 0.0));
    assert_eq!((bounds[1].x, bounds[1].y), (0.0, 0.0));
    assert_eq!((bounds[2].x, bounds[2].y), (0.0, 20.0));
}

#[test]
fn column_flow_transposes_rows_and_columns_including_occupied_cells() {
    let bounds = boxes(
        "main{display:grid;grid-template-rows:20px 30px;grid-auto-columns:40px;grid-auto-flow:column;width:120px}#fixed{grid-area:1 / 1 / 2 / 2}",
        "<main><div id=a></div><div id=fixed></div><div id=b></div></main>",
        &["a", "b"],
    );
    assert_eq!(
        (bounds[0].x, bounds[0].y, bounds[0].height),
        (0.0, 20.0, 30.0)
    );
    assert_eq!(
        (bounds[1].x, bounds[1].y, bounds[1].height),
        (40.0, 0.0, 20.0)
    );
}

#[test]
fn dense_can_backfill_a_hole_while_sparse_keeps_forward_order() {
    for (flow, expected) in [("row", (0.0, 20.0)), ("row dense", (0.0, 0.0))] {
        let bounds = boxes(
            &format!(
                "main{{display:grid;grid-template-columns:40px 40px;grid-auto-rows:20px;grid-auto-flow:{flow};width:80px}}#a{{grid-column-start:2}}"
            ),
            "<main><div id=a></div><div id=b></div></main>",
            &["b"],
        );
        assert_eq!((bounds[0].x, bounds[0].y), expected, "{flow}");
    }
}

#[test]
fn zoomed_implicit_tracks_resolve_em_rem_and_viewport_lengths() {
    let bounds = boxes(
        "html{font-size:20px}main{display:grid;grid-auto-flow:column;grid-auto-columns:2em 2rem 10vw;zoom:.5;font-size:10px;width:400px}div{height:20px}",
        "<main><div id=a></div><div id=b></div><div id=c></div></main>",
        &["a", "b", "c"],
    );
    assert_eq!(
        bounds.iter().map(|rect| rect.width).collect::<Vec<_>>(),
        [10.0, 20.0, 40.0]
    );
}

#[test]
fn out_of_range_implicit_placement_stops_at_the_ua_limit() {
    let bounds = boxes(
        "main{display:grid;grid-auto-flow:column;grid-auto-columns:1px;grid-auto-rows:1px;width:100px}#a{grid-column:1 / 2147483647;grid-row:1 / 2147483647}",
        "<main><div id=a></div><div id=b></div></main>",
        &["b"],
    );
    assert!(bounds[0].x.is_finite() && bounds[0].y.is_finite());
    assert!(bounds[0].x <= 10_000.0 && bounds[0].y <= 10_000.0);
}

#[test]
fn bounded_zero_minimum_fraction_does_not_expand_to_overflowing_contents() {
    let bounds = boxes(
        "main{display:grid;width:400px;height:100px;grid-template-rows:minmax(0,1fr)}#a{display:flex;flex-direction:column;min-height:0}#b{height:400px;flex:none}",
        "<main><div id=a><div id=b></div></div></main>",
        &["a", "b"],
    );
    assert_eq!(bounds[0].height, 100.0);
    assert_eq!(bounds[1].height, 400.0);
}

#[test]
fn two_component_grid_areas_overlap_in_the_same_track() {
    let bounds = boxes(
        "main{display:grid;width:100px;height:100px;grid-template-rows:20px minmax(0,1fr)}#a,#b{grid-area:2/1;min-height:0}#b{visibility:hidden}section{height:400px}",
        "<main><div></div><div id=a><section></section></div><div id=b><section></section></div></main>",
        &["a", "b"],
    );
    assert_eq!((bounds[0].y, bounds[0].height), (20.0, 80.0));
    assert_eq!((bounds[1].y, bounds[1].height), (20.0, 80.0));
}

#[test]
fn automatic_span_occupies_both_rows_without_becoming_a_numbered_line() {
    let bounds = boxes(
        "main{display:grid;width:100px;grid-template-columns:20px 80px;grid-template-rows:10px 10px}#icon{grid-row:span 2}#a,#b{height:10px}",
        "<main><div id=icon></div><div id=a></div><div id=b></div></main>",
        &["icon", "a", "b"],
    );
    assert_eq!(
        (bounds[0].x, bounds[0].y, bounds[0].height),
        (0.0, 0.0, 20.0)
    );
    assert_eq!((bounds[1].x, bounds[1].y), (20.0, 0.0));
    assert_eq!((bounds[2].x, bounds[2].y), (20.0, 10.0));
}

#[test]
fn negative_end_lines_and_end_anchored_spans_use_the_explicit_grid() {
    let bounds = boxes(
        "main{display:grid;width:120px;grid-template-columns:repeat(3,40px)}#a{grid-column:span 2 / -1}",
        "<main><div id=a></div></main>",
        &["a"],
    );
    assert_eq!((bounds[0].x, bounds[0].width), (40.0, 80.0));
}
