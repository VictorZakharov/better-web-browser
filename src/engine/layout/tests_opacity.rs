use super::test_support::FixedMeasurer;
use super::*;

#[test]
fn opacity_wraps_complete_nested_subtrees_without_removing_geometry() {
    let page = Page::parse(
        r#"<style>
            body { margin: 0 }
            #outer { width: 100px; height: 80px; background: red; opacity: .5 }
            #inner { width: 40px; height: 30px; background: blue; opacity: 0 }
        </style>
        <div id=outer><a href="/kept"><span id=inner>still hit testable</span></a></div>"#,
        "https://example.com/",
    );
    let outer = page
        .dom
        .elements_named("div")
        .find(|node| node.attr("id").as_deref() == Some("outer"))
        .unwrap();
    let inner = page
        .dom
        .elements_named("span")
        .find(|node| node.attr("id").as_deref() == Some("inner"))
        .unwrap();

    let output = layout_page(&page, 800.0, 600.0, &mut FixedMeasurer);
    assert_eq!(output.node_bounds[&outer.id()].width, 100.0);
    assert!(output.node_bounds[&inner.id()].width > 0.0);

    let groups = output
        .items
        .iter()
        .filter_map(|item| match item {
            DisplayItem::BeginOpacity { opacity, .. } => Some(*opacity),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(groups, [0.5, 0.0]);
    assert_eq!(
        output
            .items
            .iter()
            .filter(|item| matches!(item, DisplayItem::EndOpacity { .. }))
            .count(),
        2
    );
    assert!(output.items.iter().any(
        |item| matches!(item, DisplayItem::Text { link: Some(link), .. } if link == "https://example.com/kept")
    ));
}
