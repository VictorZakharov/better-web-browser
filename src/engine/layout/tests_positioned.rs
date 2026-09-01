use super::super::test_support::FixedMeasurer;
use super::super::*;

#[test]
fn translation_moves_paint_hit_and_cssom_geometry_without_affecting_flow() {
    let page = Page::parse(
        r#"<style>
            body { margin: 0 }
            .host { position: relative; width: 200px; height: 100px }
            .moved { position: absolute; top: 50%; width: 80px; height: 40px;
                     background: red; transform: translateY(-50%) }
            .after { width: 20px; height: 20px; background: blue }
        </style>
        <div class=host><div id=moved class=moved></div></div><div id=after class=after></div>"#,
        "https://example.com/",
    );
    let moved = page
        .dom
        .elements_named("div")
        .find(|node| node.attr("id").as_deref() == Some("moved"))
        .unwrap();
    let after = page
        .dom
        .elements_named("div")
        .find(|node| node.attr("id").as_deref() == Some("after"))
        .unwrap();

    let output = layout_page(&page, 800.0, 600.0, &mut FixedMeasurer);
    let moved_rect = output.node_bounds[&moved.id()];
    assert_eq!(moved_rect.y, 30.0);
    assert_eq!(output.node_bounds[&after.id()].y, 100.0);
    assert!(output.items.iter().any(|item| {
        matches!(item, DisplayItem::SolidRect { rect, color, .. }
            if *color == Color::rgb(255, 0, 0) && rect.y == 30.0)
    }));
}

#[test]
fn positioned_stack_levels_surround_in_flow_content_and_keep_source_order() {
    let page = Page::parse(
        r#"<style>
            body { margin: 0 }
            .host { position: relative; width: 100px; height: 100px }
            .layer { position: absolute; inset: 0; width: 100px; height: 100px }
            #minus-one { z-index: -1; background: #ff0000 }
            #minus-two { z-index: -2; background: #ffff00 }
            #normal { width: 100px; height: 100px; background: #0000ff }
            #auto { background: #00ffff }
            #zero { z-index: 0; background: #ff00ff }
            #two-first { z-index: 2; background: #00ff00 }
            #two-second { z-index: 2; background: #ffffff }
        </style>
        <div class=host>
          <div id=two-first class=layer></div>
          <div id=minus-one class=layer></div>
          <div id=normal></div>
          <div id=zero class=layer></div>
          <div id=minus-two class=layer></div>
          <div id=auto class=layer></div>
          <div id=two-second class=layer></div>
        </div>"#,
        "https://example.com/",
    );

    let output = layout_page(&page, 800.0, 600.0, &mut FixedMeasurer);
    let colors = output
        .items
        .iter()
        .filter_map(|item| match item {
            DisplayItem::SolidRect { color, .. } if color.alpha > 0 => Some(*color),
            _ => None,
        })
        .collect::<Vec<_>>();

    assert_eq!(
        colors,
        vec![
            Color::rgb(255, 255, 0),
            Color::rgb(255, 0, 0),
            Color::rgb(0, 0, 255),
            Color::rgb(255, 0, 255),
            Color::rgb(0, 255, 255),
            Color::rgb(0, 255, 0),
            Color::rgb(255, 255, 255),
        ]
    );

    let expected_nodes = [
        "minus-two",
        "minus-one",
        "normal",
        "zero",
        "auto",
        "two-first",
        "two-second",
    ]
    .map(|id| {
        page.dom
            .elements_named("div")
            .find(|node| node.attr("id").as_deref() == Some(id))
            .unwrap()
            .id()
    });
    assert!(output.node_paint_order.ends_with(&expected_nodes));
}

#[test]
fn computed_z_index_preserves_integer_and_css_wide_values() {
    let page = Page::parse(
        r#"<style>
            #parent { position: relative; z-index: 9 }
            #integer { position: absolute; z-index: -3 }
            #inherited { position: absolute; z-index: inherit }
            #initial { position: absolute; z-index: initial }
            #invalid { position: absolute; z-index: 2.5 }
        </style>
        <div id=parent>
          <div id=integer></div><div id=inherited></div>
          <div id=initial></div><div id=invalid></div>
        </div>"#,
        "https://example.com/",
    );
    let styles = page.style_for_viewport(800.0, 600.0);
    let z_index = |id: &str| {
        let node = page
            .dom
            .elements_named("div")
            .find(|node| node.attr("id").as_deref() == Some(id))
            .unwrap();
        styles.get(&node).z_index
    };

    assert_eq!(z_index("integer"), Some(-3));
    assert_eq!(z_index("inherited"), Some(9));
    assert_eq!(z_index("initial"), None);
    assert_eq!(z_index("invalid"), None);
}
