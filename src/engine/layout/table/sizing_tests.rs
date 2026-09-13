use super::*;
use crate::engine::layout::test_support::FixedMeasurer;

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
