use super::super::test_support::FixedMeasurer;
use super::super::*;

#[test]
fn relative_positioned_box_paints_after_later_normal_flow_background() {
    let page = Page::parse(
        "<style>body{margin:0}#player{position:relative;z-index:1;height:0}#frame{position:absolute;width:100px;height:100px;background:red}#app{height:100px;background:white}</style><div id=player><div id=frame></div></div><div id=app></div>",
        "https://example.com/",
    );
    let output = layout_page(&page, 800.0, 600.0, &mut FixedMeasurer);
    let colors = output
        .items
        .iter()
        .filter_map(|item| match item {
            DisplayItem::SolidRect { color, .. } => Some(*color),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(colors, vec![Color::WHITE, Color::rgb(255, 0, 0)]);
    let frame = page
        .dom
        .elements_named("div")
        .find(|node| node.attr("id").as_deref() == Some("frame"))
        .unwrap();
    assert_eq!(output.node_paint_order.last(), Some(&frame.id()));
}

#[test]
fn relative_and_absolute_stack_levels_share_source_order_and_keep_flow_geometry() {
    let page = Page::parse(
        "<style>body{margin:0}#negative{position:relative;z-index:-1;height:10px;background:red}#normal{height:10px;background:blue}#absolute{position:absolute;z-index:0;width:10px;height:10px;background:cyan}#relative{position:relative;z-index:0;height:10px;background:lime}</style><div id=negative></div><div id=normal></div><div id=absolute></div><div id=relative></div>",
        "https://example.com/",
    );
    let output = layout_page(&page, 800.0, 600.0, &mut FixedMeasurer);
    let colors = output
        .items
        .iter()
        .filter_map(|item| match item {
            DisplayItem::SolidRect { color, .. } => Some(*color),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        colors,
        vec![
            Color::rgb(255, 0, 0),
            Color::rgb(0, 0, 255),
            Color::rgb(0, 255, 255),
            Color::rgb(0, 255, 0)
        ]
    );
    let normal = page
        .dom
        .elements_named("div")
        .find(|node| node.attr("id").as_deref() == Some("normal"))
        .unwrap();
    assert_eq!(output.node_bounds[&normal.id()].y, 10.0);
}

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
fn positioned_start_and_end_insets_include_the_corresponding_margin() {
    let page = Page::parse(
        r#"<style>
            body { margin: 0 }
            #start { position: fixed; left: 5px; top: 20px; margin: 3px 0 0 5px;
                     width: 20px; height: 20px; transform: translateX(10px) }
            #end { position: fixed; right: 10px; top: 0; margin-right: 7px;
                   width: 20px; height: 20px }
        </style>
        <div id=start></div><div id=end></div>"#,
        "https://example.com/",
    );
    let node = |id: &str| {
        page.dom
            .elements_named("div")
            .find(|node| node.attr("id").as_deref() == Some(id))
            .unwrap()
    };

    let output = layout_page(&page, 800.0, 600.0, &mut FixedMeasurer);

    assert_eq!(output.node_bounds[&node("start").id()].x, 20.0);
    assert_eq!(output.node_bounds[&node("start").id()].y, 23.0);
    assert_eq!(output.node_bounds[&node("end").id()].x, 763.0);
}

#[test]
fn empty_inline_box_keeps_its_line_position_inside_nested_positioned_boxes() {
    let page = Page::parse(
        r#"<style>
            body { margin: 0 }
            .square { width: 10px; height: 10px }
            #one { position: absolute; top: 10px; left: 10px }
            #two { position: absolute; top: 50px; left: 50px }
            span.square { display: inline-block }
        </style>
        <div id=one class=square><div id=two class=square>
            <div class=square></div><span class=square></span><span id=target></span>
        </div></div>"#,
        "https://example.com/",
    );
    let target = page
        .dom
        .elements_named("span")
        .find(|node| node.attr("id").as_deref() == Some("target"))
        .unwrap();

    let output = layout_page(&page, 800.0, 600.0, &mut FixedMeasurer);

    assert_eq!(
        output.node_bounds[&target.id()],
        RectF {
            x: 70.0,
            y: 70.0,
            width: 0.0,
            height: 0.0,
        }
    );
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
