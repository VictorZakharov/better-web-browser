use super::*;
use crate::engine::layout::test_support::FixedMeasurer;

#[test]
fn explicit_table_height_is_allocated_to_empty_row_boxes() {
    let page = Page::parse(
        "<style>body{margin:0}table{width:200px;height:200px;border-spacing:0}td{background:blue}</style><table><tr><td></td><td></td></tr><tr><td></td><td></td></tr></table>",
        "https://example.test/",
    );
    let output = layout_page(&page, 800.0, 600.0, &mut FixedMeasurer);
    let cells = page.dom.elements_named("td").collect::<Vec<_>>();
    let rects = cells
        .iter()
        .map(|cell| output.node_bounds[&cell.id()])
        .collect::<Vec<_>>();
    assert_eq!(rects[0].height, 100.0);
    assert_eq!(rects[1].height, 100.0);
    assert_eq!(rects[2].y, 100.0);
    assert_eq!(rects[3].y, 100.0);
    assert_eq!(rects[2].bottom(), 200.0);
}

#[test]
fn separated_border_spacing_places_cells_and_preserves_outer_gaps() {
    let page = Page::parse(
        "<style>body{margin:0}table{width:120px;height:90px;border-spacing:4px 6px}td{padding:0}</style>\
         <table><tr><td>A</td><td>B</td></tr><tr><td>C</td><td>D</td></tr></table>",
        "https://example.test/",
    );
    let output = layout_page(&page, 800.0, 600.0, &mut FixedMeasurer);
    let table = page.dom.elements_named("table").next().unwrap();
    let table = output.node_bounds[&table.id()];
    let cells = page.dom.elements_named("td").collect::<Vec<_>>();
    let bounds = cells
        .iter()
        .map(|cell| output.node_bounds[&cell.id()])
        .collect::<Vec<_>>();
    assert_eq!(table.width, 120.0);
    assert_eq!(bounds[0].x - table.x, 4.0);
    assert_eq!(bounds[1].x - bounds[0].right(), 4.0);
    assert_eq!(table.right() - bounds[1].right(), 4.0);
    assert_eq!(bounds[0].y - table.y, 6.0);
    assert_eq!(bounds[2].y - bounds[0].bottom(), 6.0);
    assert_eq!(table.bottom() - bounds[2].bottom(), 6.0);
}

#[test]
fn spanning_cell_includes_only_interior_border_spacing() {
    let page = Page::parse(
        "<style>body{margin:0}table{width:120px;border-spacing:4px 6px}td{padding:0}</style>\
         <table><tr><td colspan=2>wide</td></tr><tr><td>A</td><td>B</td></tr></table>",
        "https://example.test/",
    );
    let output = layout_page(&page, 800.0, 600.0, &mut FixedMeasurer);
    let cells = page.dom.elements_named("td").collect::<Vec<_>>();
    let bounds = cells
        .iter()
        .map(|cell| output.node_bounds[&cell.id()])
        .collect::<Vec<_>>();
    assert_eq!(bounds[0].x, bounds[1].x);
    assert_eq!(bounds[0].right(), bounds[2].right());
    assert_eq!(bounds[0].width, bounds[1].width + bounds[2].width + 4.0);
}

#[test]
fn collapse_suppresses_border_spacing_without_changing_computed_value() {
    let page = Page::parse(
        "<style>body{margin:0}table{width:100px;border-spacing:12px 9px;border-collapse:collapse}</style>\
         <table><tr><td>A</td><td>B</td></tr></table>",
        "https://example.test/",
    );
    let output = layout_page(&page, 800.0, 600.0, &mut FixedMeasurer);
    let cells = page.dom.elements_named("td").collect::<Vec<_>>();
    let left = output.node_bounds[&cells[0].id()];
    let right = output.node_bounds[&cells[1].id()];
    assert_eq!(left.x, 0.0);
    assert_eq!(right.x, left.right());
}

#[test]
fn html_table_default_gaps_and_cell_padding_are_visible_in_geometry() {
    let page = Page::parse(
        "<style>body{margin:0}table{width:100px}</style><table><tr><td>A</td><td>B</td></tr></table>",
        "https://example.test/",
    );
    let output = layout_page(&page, 800.0, 600.0, &mut FixedMeasurer);
    let table = page.dom.elements_named("table").next().unwrap();
    let bounds = output.node_bounds[&table.id()];
    let cells = page.dom.elements_named("td").collect::<Vec<_>>();
    let first = output.node_bounds[&cells[0].id()];
    let second = output.node_bounds[&cells[1].id()];
    assert_eq!(first.x, bounds.x + 2.0);
    assert_eq!(second.x, first.right() + 2.0);
    assert_eq!(second.right(), bounds.right() - 2.0);
    assert_eq!(first.y, bounds.y + 2.0);
    assert_eq!(bounds.bottom(), first.bottom() + 2.0);
}

#[test]
fn html_cellspacing_and_cellpadding_hints_affect_box_geometry() {
    let page = Page::parse(
        "<style>body{margin:0}table{width:100px}</style>\
         <table cellspacing=5 cellpadding=4><tr><td>A</td><td>B</td></tr></table>",
        "https://example.test/",
    );
    let output = layout_page(&page, 800.0, 600.0, &mut FixedMeasurer);
    let table = page.dom.elements_named("table").next().unwrap();
    let bounds = output.node_bounds[&table.id()];
    let cells = page.dom.elements_named("td").collect::<Vec<_>>();
    let first = output.node_bounds[&cells[0].id()];
    let second = output.node_bounds[&cells[1].id()];
    assert_eq!(first.x, bounds.x + 5.0);
    assert_eq!(second.x, first.right() + 5.0);
    assert_eq!(second.right(), bounds.right() - 5.0);
    assert!(
        first.height > 20.0,
        "cellpadding must enlarge the box: {first:?}"
    );
}

#[test]
fn explicit_table_height_preserves_intrinsic_row_minima() {
    let page = Page::parse(
        "<style>body{margin:0}table{width:200px;height:150px;border-spacing:0}td{padding:0}</style><table><tr style='height:100px'><td>first</td></tr><tr><td>last</td></tr></table>",
        "https://example.test/",
    );
    let output = layout_page(&page, 800.0, 600.0, &mut FixedMeasurer);
    let cells = page.dom.elements_named("td").collect::<Vec<_>>();
    let first = output.node_bounds[&cells[0].id()];
    let second = output.node_bounds[&cells[1].id()];
    assert!(first.height >= 100.0, "{first:?}");
    assert_eq!(first.bottom(), second.y);
    assert_eq!(second.bottom(), 150.0);
}

#[test]
fn percentage_descendants_cannot_inflate_the_table_track_they_depend_on() {
    for child in [
        "<div style='width:100%'>short text</div>",
        "<div style='width:100%'><i style='margin-left:auto;margin-right:auto'>An ordinary inline with automatic margins must still wrap inside the allocated table cell.</i></div>",
        "<div style='width:calc(100% - 2px)'>short text</div>",
        "<table style='width:100%'><tr><td>short text</td></tr></table>",
    ] {
        let page = Page::parse(
            &format!(
                "<style>body{{margin:0}}table{{width:320px;box-sizing:border-box;padding:3px;border:1px solid}}\
                 td{{padding:4px 8px}}</style><table><tr><td>{child}</td></tr></table>"
            ),
            "https://example.test/",
        );
        let output = layout_page(&page, 800.0, 600.0, &mut FixedMeasurer);
        let table = page.dom.elements_named("table").next().unwrap();
        let cell = page.dom.elements_named("td").next().unwrap();
        let table = output.node_bounds[&table.id()];
        let cell = output.node_bounds[&cell.id()];
        assert!((table.width - 320.0).abs() < 0.01, "{child}: {table:?}");
        assert!(
            cell.right() <= table.right(),
            "{child}: {cell:?} outside {table:?}"
        );
    }
}

#[test]
fn cell_height_is_a_minimum_that_cannot_clip_wrapped_content() {
    for display in ["table-cell", "block"] {
        let page = Page::parse(
            &format!(
                "<style>body{{margin:0}}table{{width:160px}}\
                 th,td{{padding:2px;border:1px solid;line-height:20px;font-size:16px}}\
                 th{{height:16px;display:{display}}}</style>\
                 <table><tr><th>Long header<br>second line<br>third line</th>\
                 <td>value</td></tr><tr><td id=next>next row</td></tr></table>"
            ),
            "https://example.test/",
        );
        let output = layout_page(&page, 800.0, 600.0, &mut FixedMeasurer);
        let geometry =
            layout_geometry_with_style_viewport(&page, 800.0, 600.0, 800.0, &mut FixedMeasurer);
        assert_eq!(output.node_bounds, geometry.node_bounds);
        assert_eq!(output.resize_boxes, geometry.resize_boxes);
        let header = page.dom.elements_named("th").next().unwrap();
        let bounds = output.node_bounds[&header.id()];
        let next = Node::descendants(&page.dom.document)
            .find(|node| node.attr("id").as_deref() == Some("next"))
            .unwrap();
        let next = output.node_bounds[&next.id()];
        if display == "table-cell" {
            assert!(bounds.height >= 66.0, "wrapped cell must grow: {bounds:?}");
            for item in &output.items {
                if let DisplayItem::Text { rect, text, .. } = item
                    && matches!(text.trim(), "third" | "line")
                {
                    assert!(
                        rect.bottom() <= next.y,
                        "{text}: {rect:?} overlaps {next:?}"
                    );
                }
            }
        } else {
            assert_eq!(bounds.height, 26.0, "ordinary blocks retain fixed height");
        }
    }
}

#[test]
fn taller_specified_cell_height_stretches_every_cell_and_moves_the_next_row() {
    let page = Page::parse(
        "<style>body{margin:0}table{width:160px;border-spacing:0}td{padding:2px;border:1px solid;\
         line-height:20px;font-size:16px}</style><table>\
         <tr><td style='height:100px'>short</td><td>peer</td></tr>\
         <tr><td>next</td></tr></table>",
        "https://example.test/",
    );
    let output = layout_page(&page, 800.0, 600.0, &mut FixedMeasurer);
    let cells = page.dom.elements_named("td").collect::<Vec<_>>();
    assert_eq!(output.node_bounds[&cells[0].id()].height, 106.0);
    assert_eq!(output.node_bounds[&cells[1].id()].height, 106.0);
    assert_eq!(output.node_bounds[&cells[2].id()].y, 106.0);
}

#[test]
fn percentage_text_cell_does_not_starve_image_and_padding() {
    let page = Page::parse(
        r#"<style>
      body {margin:0} table {width:400px;border-spacing:0} td {padding:4px 8px}
      .message {width:100%} .icon {padding:2px 4px}
    </style><table><tr><td class=icon><img width=52 height=40></td>
      <td class=message>A notice which must wrap beside the icon, never under it.</td>
    </tr></table>"#,
        "https://example.test/",
    );
    let output = layout_page(&page, 800.0, 600.0, &mut FixedMeasurer);
    let cells = page.dom.elements_named("td").collect::<Vec<_>>();
    let icon = output.node_bounds[&cells[0].id()];
    let message = output.node_bounds[&cells[1].id()];
    assert_eq!(icon.width, 60.0);
    assert_eq!(message.x, icon.right());
    assert_eq!(message.right(), 400.0);
    assert_eq!(
        message.height, icon.height,
        "cell boxes fill the shared row height"
    );
    let image = output.node_bounds[&page.dom.elements_named("img").next().unwrap().id()];
    assert_eq!(image.x, 4.0);
    assert!((image.y - (icon.height - image.height) / 2.0).abs() < 0.001);
    let text = output
        .items
        .iter()
        .find_map(|item| match item {
            DisplayItem::Text { rect, .. } => Some(rect),
            _ => None,
        })
        .unwrap();
    assert_eq!(text.x, message.x + 8.0);
}
