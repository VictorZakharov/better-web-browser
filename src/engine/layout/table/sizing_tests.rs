use super::*;
use crate::engine::layout::test_support::FixedMeasurer;

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
        "<style>body{margin:0}table{width:160px}td{padding:2px;border:1px solid;\
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
      body {margin:0} table {width:400px} td {padding:4px 8px}
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
    assert_eq!((image.x, image.y), (4.0, 2.0));
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
