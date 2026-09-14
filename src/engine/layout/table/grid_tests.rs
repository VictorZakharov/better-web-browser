use super::*;
use crate::engine::layout::test_support::FixedMeasurer;

#[test]
fn css_rows_and_cells_participate_without_html_table_tags_or_spans() {
    let (page, output) = layout(
        "<style>main{display:table;width:300px}section{display:table-row}span{display:table-cell;padding:0}</style><main><section><span id=a colspan=2><img width=80 height=60></span><span id=b><img width=40 height=100></span></section><section><span id=c>label</span><span id=d>value</span></section></main>",
    );
    let rect = |id| bounds(&page, &output, id);
    assert!(rect("a").width >= 80.0);
    assert!(rect("b").width >= 40.0);
    assert_eq!(rect("a").height, 100.0);
    assert_eq!(rect("b").height, 100.0);
    assert_eq!(rect("a").width, rect("c").width);
    assert_eq!(rect("b").x, rect("d").x);
    assert_eq!(rect("a").right(), rect("b").x);
    assert_eq!(rect("b").right(), 300.0);
    assert_eq!(rect("c").y, rect("a").bottom());
}

fn layout(markup: &str) -> (Page, LayoutOutput) {
    let page = Page::parse(
        &format!(
            "<style>body{{margin:0}}td,th{{padding:0;border:0;line-height:20px}}table{{width:300px}}</style>{markup}"
        ),
        "https://example.test/",
    );
    let output = layout_page(&page, 800.0, 600.0, &mut FixedMeasurer);
    let geometry =
        layout_geometry_with_style_viewport(&page, 800.0, 600.0, 800.0, &mut FixedMeasurer);
    assert_eq!(output.node_bounds, geometry.node_bounds);
    assert_eq!(output.resize_boxes, geometry.resize_boxes);
    (page, output)
}

fn bounds(page: &Page, output: &LayoutOutput, id: &str) -> RectF {
    let node = Node::descendants(&page.dom.document)
        .find(|node| node.attr("id").as_deref() == Some(id))
        .unwrap();
    output.node_bounds[&node.id()]
}

#[test]
fn all_rows_share_columns_even_when_their_intrinsic_widths_differ() {
    let (page, output) = layout(
        "<table><tr><th id=a>Month</th><td id=b>Jan</td></tr><tr><th id=c>Mean daily maximum</th><td id=d>32.5<br>(90.5)</td></tr><tr><td id=e>Source</td></tr></table>",
    );
    let rect = |id| bounds(&page, &output, id);
    assert_eq!(rect("a").width, rect("c").width);
    assert_eq!(rect("c").width, rect("e").width);
    assert_eq!(rect("b").x, rect("d").x);
    assert_eq!(rect("b").right(), rect("d").right());
    assert!(rect("c").width > rect("d").width);
    assert_eq!(rect("d").right(), 300.0);
}

#[test]
fn spanning_header_and_footer_use_the_same_grid_as_data_cells() {
    let (page, output) = layout(
        "<table><tr><th id=title colspan=3>A long heading across three shared columns</th></tr><tr><td id=a>A</td><td id=b>BBBB</td><td id=c>CC</td></tr><tr><td id=d colspan=2>paired</td><td id=e>end</td></tr><tr><td id=f colspan=3>footer</td></tr></table>",
    );
    let rect = |id| bounds(&page, &output, id);
    assert_eq!(rect("title").width, 300.0);
    assert_eq!(rect("f").width, 300.0);
    assert_eq!(rect("d").right(), rect("c").x);
    assert_eq!(rect("e").x, rect("c").x);
    assert_eq!(rect("d").y, rect("a").bottom());
}

#[test]
fn rowspans_reserve_columns_and_distribute_their_minimum_height() {
    let (page, output) = layout(
        "<table><tr><td id=span rowspan=2 style='height:100px'>group</td><td id=a>A</td></tr><tr><td id=b>B</td></tr><tr><td id=c>C</td><td>D</td></tr></table>",
    );
    let rect = |id| bounds(&page, &output, id);
    assert_eq!(rect("a").x, rect("span").right());
    assert_eq!(rect("a").x, rect("b").x);
    assert_eq!(rect("span").height, 100.0);
    assert_eq!(rect("b").bottom(), rect("span").bottom());
    assert_eq!(rect("c").y, rect("span").bottom());
    assert_eq!(rect("c").x, 0.0);
}

#[test]
fn zero_rowspan_stops_at_its_row_group_and_hidden_cells_do_not_reserve_slots() {
    let (page, output) = layout(
        "<table><tbody><tr><td id=span rowspan=0>group</td><td id=a>A</td></tr><tr><td style='display:none' colspan=10 rowspan=5>hidden</td><td id=b>B</td></tr></tbody><tbody><tr><td id=c>C</td><td>D</td></tr></tbody></table>",
    );
    let rect = |id| bounds(&page, &output, id);
    assert_eq!(rect("a").x, rect("b").x);
    assert_eq!(rect("span").bottom(), rect("b").bottom());
    assert_eq!(rect("c").x, 0.0);
    assert_eq!(rect("c").width, rect("span").width);
}

#[test]
fn auto_table_uses_shared_max_content_width_instead_of_filling_its_container() {
    let (page, output) = layout(
        "<table id=table style='width:auto'><tr><td>AAAA</td><td>BB</td></tr><tr><td>A</td><td>BBBBBB</td></tr></table>",
    );
    assert_eq!(bounds(&page, &output, "table").width, 80.0);
}

#[test]
fn spanning_minimum_cannot_be_clipped_by_a_smaller_authored_table_width() {
    let (page, output) = layout(
        "<table id=table style='width:20px'><tr><td colspan=2>unbreakable</td></tr><tr><td id=a>A</td><td id=b>B</td></tr></table>",
    );
    assert_eq!(bounds(&page, &output, "table").width, 88.0);
    assert_eq!(bounds(&page, &output, "b").right(), 88.0);
}

#[test]
fn html_span_parsing_is_clamped_and_accepts_integer_prefixes() {
    let page = Page::parse(
        "<table><tr><td colspan=' +2junk'></td><td colspan='-2'></td><td colspan=0></td><td colspan=9999999999999999999999999></td></tr></table>",
        "https://example.test/",
    );
    let styles = page.style_for_viewport(800.0, 600.0);
    let table = page.dom.elements_named("table").next().unwrap();
    let grid = grid::Grid::new(&table, &styles);
    assert_eq!(
        grid.cells
            .iter()
            .map(|cell| cell.columns)
            .collect::<Vec<_>>(),
        [2, 1, 1, 1000]
    );
    assert_eq!(grid.columns, 1004);
}
