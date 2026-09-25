use super::*;

#[test]
fn html_root_has_a_box_and_body_margin_keeps_its_offset() {
    let page = Page::parse(
        "<style>html{width:100vw;height:100vh}</style><p>content</p>",
        "https://example.com/",
    );
    let html = page.dom.elements_named("html").next().unwrap();
    let body = page.dom.elements_named("body").next().unwrap();
    let output = layout_page(&page, 800.0, 600.0, &mut FixedMeasurer);

    assert_eq!(output.node_bounds[&html.id()].x, 0.0);
    assert_eq!(output.node_bounds[&html.id()].y, 0.0);
    assert_eq!(output.node_bounds[&html.id()].width, 800.0);
    assert_eq!(output.node_bounds[&html.id()].height, 600.0);
    assert_eq!(output.node_bounds[&body.id()].x, 8.0);
}

#[test]
fn hidden_block_keeps_geometry_and_paints_visible_descendant_only() {
    let page = Page::parse(
        r#"<style>
            body { margin: 0 }
            #hidden { visibility: hidden; width: 120px; height: 70px;
                background: red; border: 2px solid red }
            #visible { visibility: visible; width: 40px; height: 20px;
                background: blue }
            #after { width: 30px; height: 10px }
        </style><div id="hidden"><div id="visible"></div></div><div id="after"></div>"#,
        "https://example.com/",
    );
    let hidden = page.dom.elements_named("div").next().unwrap();
    let visible = page.dom.elements_named("div").nth(1).unwrap();
    let after = page.dom.elements_named("div").nth(2).unwrap();
    let mut measurer = FixedMeasurer;
    let output = layout_page(&page, 800.0, 600.0, &mut measurer);

    assert_eq!(output.node_bounds[&hidden.id()].width, 124.0);
    assert_eq!(output.node_bounds[&hidden.id()].height, 74.0);
    assert_eq!(output.node_bounds[&visible.id()].width, 40.0);
    assert_eq!(output.node_bounds[&after.id()].y, 74.0);
    assert!(output.items.iter().any(|item| {
        matches!(item, DisplayItem::SolidRect { color, .. } if *color == Color::rgb(0, 0, 255))
    }));
    assert!(!output.items.iter().any(|item| {
        matches!(item, DisplayItem::SolidRect { color, .. } if *color == Color::rgb(255, 0, 0))
    }));
}

#[test]
fn fixed_zero_area_hidden_target_still_has_position() {
    let page = Page::parse(
        r#"<style>#target { width: 0; height: 0; position: fixed; top: -1000px }</style>
           <div id="target"></div>"#,
        "https://example.com/",
    );
    let target = page.dom.elements_named("div").next().unwrap();
    let mut measurer = FixedMeasurer;
    let output = layout_page(&page, 800.0, 600.0, &mut measurer);

    assert_eq!(
        output.node_bounds.get(&target.id()).map(|rect| rect.x),
        Some(8.0)
    );
    assert_eq!(
        output.node_bounds.get(&target.id()).map(|rect| rect.y),
        Some(-1000.0)
    );
}
