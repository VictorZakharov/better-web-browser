use super::test_support::FixedMeasurer;
use super::*;

#[test]
fn text_input_auto_height_does_not_duplicate_css_padding() {
    let page = Page::parse(
        r#"<style>
            body { margin: 0 }
            input { display: block; font-size: 16px; line-height: 20px;
                    padding: 1px 0; border-width: 1px; }
        </style><input id="search" placeholder="Search">"#,
        "https://example.com/",
    );
    let input = page.dom.elements_named("input").next().unwrap();
    let mut measurer = FixedMeasurer;
    let output = layout_page(&page, 800.0, 600.0, &mut measurer);

    let control = output
        .items
        .iter()
        .find_map(|item| match item {
            DisplayItem::Control(spec) if spec.node_id == input.id() => Some(spec),
            _ => None,
        })
        .unwrap();
    assert_eq!(control.rect.height, 24.0);
    assert_eq!(output.node_bounds[&input.id()].height, 24.0);
}
