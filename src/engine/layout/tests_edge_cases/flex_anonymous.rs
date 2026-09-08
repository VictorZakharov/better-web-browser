use super::super::test_support::FixedMeasurer;
use super::super::*;

#[test]
fn direct_text_does_not_flatten_element_flex_items() {
    let mut page = Page::parse(
        r#"<main style="display:flex;flex-direction:column;width:300px">
              direct text
              <x-feed id="feed"><section id="grid" style="display:flex;height:40px">
                <div style="width:100px;background:#f00">card</div>
              </section></x-feed>
            </main>"#,
        "https://example.com/",
    );
    let host = page.dom.elements_named("x-feed").next().unwrap();
    let root = Node::attach_shadow(
        &host,
        crate::engine::dom::ShadowRootMode::Open,
        false,
        false,
        false,
    )
    .unwrap();
    Node::replace_inner_html(&root, "<slot></slot>", true);
    page.refresh_resources(800.0);

    let feed = page
        .dom
        .elements_named("x-feed")
        .find(|node| node.attr("id").as_deref() == Some("feed"))
        .unwrap();
    let grid = page
        .dom
        .elements_named("section")
        .find(|node| node.attr("id").as_deref() == Some("grid"))
        .unwrap();
    let mut measurer = FixedMeasurer;
    let output = layout_page(&page, 800.0, 600.0, &mut measurer);

    assert!(output.node_bounds.contains_key(&feed.id()));
    assert_eq!(output.node_bounds.get(&grid.id()).unwrap().height, 40.0);
}

#[test]
fn inline_shadow_host_does_not_flatten_block_descendants_into_text() {
    let mut page = Page::parse(
        r#"<main><x-feed><section id="feed" style="display:flex;height:40px;width:300px">
              <div style="width:100px;background:#f00">card</div>
            </section></x-feed><p id="after">After</p></main>"#,
        "https://example.com/",
    );
    let host = page.dom.elements_named("x-feed").next().unwrap();
    let root = Node::attach_shadow(
        &host,
        crate::engine::dom::ShadowRootMode::Open,
        false,
        false,
        false,
    )
    .unwrap();
    Node::replace_inner_html(
        &root,
        r#"<div id="holder" style="display:block"><slot></slot></div>"#,
        true,
    );
    page.refresh_resources(800.0);

    let holder = Node::descendants(&root)
        .find(|node| node.attr("id").as_deref() == Some("holder"))
        .unwrap();
    let feed = page
        .dom
        .elements_named("section")
        .find(|node| node.attr("id").as_deref() == Some("feed"))
        .unwrap();
    let after = page
        .dom
        .elements_named("p")
        .find(|node| node.attr("id").as_deref() == Some("after"))
        .unwrap();
    let mut measurer = FixedMeasurer;
    let output = layout_page(&page, 800.0, 600.0, &mut measurer);

    assert_eq!(output.node_bounds.get(&feed.id()).unwrap().height, 40.0);
    assert_eq!(output.node_bounds.get(&holder.id()).unwrap().height, 40.0);
    assert!(output.node_bounds.get(&after.id()).unwrap().y >= 48.0);
}

#[test]
fn zero_flex_basis_still_contributes_max_content_to_an_auto_sized_ancestor() {
    let page = Page::parse(
        r#"<body style="margin:0">
            <div style="display:flex">
              <x-control id="control" style="display:flex">
                <x-shape id="shape" style="display:flex;flex:1 1 0%;min-width:0">
                  <a style="display:flex;height:40px;padding:0 15px;gap:6px">
                    <span id="icon" style="display:block;width:24px;height:24px"></span>
                    <span id="label" style="display:block;white-space:nowrap">Sign in</span>
                  </a>
                </x-shape>
              </x-control>
            </div>
        </body>"#,
        "https://example.com/",
    );
    let by_id = |id: &str| {
        Node::descendants(&page.dom.document)
            .find(|node| node.attr("id").as_deref() == Some(id))
            .unwrap()
    };
    let control = by_id("control");
    let shape = by_id("shape");
    let icon = by_id("icon");
    let label = by_id("label");
    let mut measurer = FixedMeasurer;
    let output = layout_page(&page, 800.0, 600.0, &mut measurer);

    assert!(output.node_bounds[&control.id()].width >= 116.0);
    assert!(output.node_bounds[&shape.id()].width >= 116.0);
    assert_eq!(output.node_bounds[&icon.id()].width, 24.0);
    assert!(output.node_bounds[&label.id()].width > 1.0);
}

#[test]
fn column_flex_centers_a_fixed_width_item_on_the_cross_axis() {
    let page = Page::parse(
        r#"<main style="display:flex;flex-flow:column;align-items:center;width:300px">
              <section id="item" style="width:100px;height:20px"></section>
            </main>"#,
        "https://example.com/",
    );
    let item = page
        .dom
        .elements_named("section")
        .find(|node| node.attr("id").as_deref() == Some("item"))
        .unwrap();
    let mut measurer = FixedMeasurer;
    let output = layout_page(&page, 800.0, 600.0, &mut measurer);

    assert_eq!(
        output.node_bounds.get(&item.id()),
        Some(&RectF {
            x: 108.0,
            y: 8.0,
            width: 100.0,
            height: 20.0,
        })
    );
}

#[test]
fn row_flex_cross_axis_alignment_translates_paint_and_dom_geometry_together() {
    let page = Page::parse(
        r#"<body style="margin:0">
            <main style="display:flex;align-items:center;width:300px;height:56px">
              <section id="item" style="width:100px;height:40px;background:#f00">
                <div id="descendant" style="height:10px"></div>
              </section>
            </main>
        </body>"#,
        "https://example.com/",
    );
    let item = page
        .dom
        .elements_named("section")
        .find(|node| node.attr("id").as_deref() == Some("item"))
        .unwrap();
    let descendant = page
        .dom
        .elements_named("div")
        .find(|node| node.attr("id").as_deref() == Some("descendant"))
        .unwrap();
    let mut measurer = FixedMeasurer;
    let output = layout_page(&page, 800.0, 600.0, &mut measurer);

    assert_eq!(output.node_bounds[&item.id()].y, 8.0);
    assert_eq!(output.node_bounds[&descendant.id()].y, 8.0);
    assert!(output.items.iter().any(|display_item| {
        matches!(display_item, DisplayItem::SolidRect { rect, color, .. }
            if rect.y == 8.0 && *color == Color::rgb(255, 0, 0))
    }));
}

#[test]
fn reverse_flex_directions_reverse_main_start_and_visual_order() {
    let page = Page::parse(
        r#"<body style="margin:0">
            <main style="display:flex;flex-direction:row-reverse;width:300px">
              <section id="row-first" style="width:50px;height:20px"></section>
              <section id="row-second" style="width:50px;height:20px"></section>
            </main>
            <main style="display:flex;flex-direction:column-reverse;width:100px;height:100px">
              <section id="column-first" style="height:20px"></section>
              <section id="column-second" style="height:20px"></section>
            </main>
        </body>"#,
        "https://example.com/",
    );
    let by_id = |id: &str| {
        Node::descendants(&page.dom.document)
            .find(|node| node.attr("id").as_deref() == Some(id))
            .unwrap()
    };
    let row_first = by_id("row-first");
    let row_second = by_id("row-second");
    let column_first = by_id("column-first");
    let column_second = by_id("column-second");
    let mut measurer = FixedMeasurer;
    let output = layout_page(&page, 800.0, 600.0, &mut measurer);

    assert_eq!(output.node_bounds[&row_first.id()].x, 250.0);
    assert_eq!(output.node_bounds[&row_second.id()].x, 200.0);
    assert_eq!(output.node_bounds[&column_first.id()].y, 100.0);
    assert_eq!(output.node_bounds[&column_second.id()].y, 80.0);
}

#[test]
fn row_flex_stretches_auto_cross_sizes_through_normal_and_replaced_layout() {
    let page = Page::parse(
        r#"<body style="margin:0">
            <main style="display:flex;align-items:stretch;width:300px;height:40px">
              <section id="item" style="width:100px">
                <div id="percentage-child" style="height:100%"></div>
              </section>
              <button id="control" style="width:64px;padding:0;border-width:0">Go</button>
            </main>
        </body>"#,
        "https://example.com/",
    );
    let by_id = |id: &str| {
        Node::descendants(&page.dom.document)
            .find(|node| node.attr("id").as_deref() == Some(id))
            .unwrap()
    };
    let item = by_id("item");
    let percentage_child = by_id("percentage-child");
    let control = by_id("control");
    let mut measurer = FixedMeasurer;
    let output = layout_page(&page, 800.0, 600.0, &mut measurer);

    assert_eq!(output.node_bounds[&item.id()].height, 40.0);
    assert_eq!(output.node_bounds[&percentage_child.id()].height, 40.0);
    assert!(output.items.iter().any(|display_item| {
        matches!(display_item, DisplayItem::Control(spec)
            if spec.node_id == control.id() && spec.rect.height == 40.0)
    }));
}
