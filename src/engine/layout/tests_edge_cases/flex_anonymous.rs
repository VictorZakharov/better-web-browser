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
