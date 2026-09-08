use super::*;
use crate::engine::dom::ShadowRootMode;
use crate::engine::invalidation::RenderInvalidation;
use crate::engine::layout::test_support::FixedMeasurer;

fn slotted_page(shadow_markup: &str) -> Page {
    let page = Page::parse(
        r#"<style>body{margin:0}x-host{display:block}</style>
        <x-host>
            <div id=assigned slot=content style='display:block;width:20px;height:10px;background:blue'>
                <b id=nested style='display:block;width:5px;height:4px;background:red'></b>
            </div>
            <div id=unassigned slot=unused style='display:block;width:500px;height:500px'></div>
        </x-host><aside id=outside style='display:block;width:5px;height:5px'></aside>"#,
        "https://example.test/",
    );
    let host = page.dom.elements_named("x-host").next().unwrap();
    let root = Node::attach_shadow(&host, ShadowRootMode::Open, false, false, false).unwrap();
    Node::replace_inner_html(&root, shadow_markup, true);
    page
}

fn identified(page: &Page, id: &str) -> NodeId {
    Node::shadow_including_descendants(&page.dom.document)
        .find(|node| node.attr("id").as_deref() == Some(id))
        .unwrap()
        .id()
}

fn assert_boxes_match_paint(page: &Page, x: f32, y: f32, outside_y: f32) {
    let assigned = identified(page, "assigned");
    let nested = identified(page, "nested");
    let outside = identified(page, "outside");
    let unassigned = identified(page, "unassigned");
    let fallback = identified(page, "fallback");
    let retained = layout_page(page, 800.0, 600.0, &mut FixedMeasurer);
    assert_eq!(
        retained.node_bounds[&assigned],
        RectF {
            x,
            y,
            width: 20.0,
            height: 10.0
        }
    );
    assert_eq!(
        retained.node_bounds[&nested],
        RectF {
            x,
            y,
            width: 5.0,
            height: 4.0
        }
    );
    assert_eq!(
        retained.node_bounds[&outside].y, outside_y,
        "translations do not alter normal flow"
    );
    assert!(!retained.node_bounds.contains_key(&unassigned));
    assert!(!retained.node_bounds.contains_key(&fallback));
    for (color, node) in [
        (Color::rgb(0, 0, 255), assigned),
        (Color::rgb(255, 0, 0), nested),
    ] {
        assert!(
            retained.items.iter().any(|item| matches!(item,
                DisplayItem::SolidRect { rect, color:actual, .. }
                    if *actual == color && *rect == retained.node_bounds[&node]
            )),
            "paint and CSSOM geometry agree for {color:?}"
        );
    }
    assert_eq!(
        layout_geometry_with_style_viewport(page, 800.0, 600.0, 800.0, &mut FixedMeasurer),
        retained.node_bounds
    );
    let mut sparse = page.layout_snapshot();
    sparse.refresh_layout_styles_after_invalidation_for_viewport(
        800.0,
        600.0,
        &RenderInvalidation::full(page.dom.document.id()),
    );
    assert_eq!(
        layout_geometry_with_style_viewport(&sparse, 800.0, 600.0, 800.0, &mut FixedMeasurer),
        retained.node_bounds
    );
}

#[test]
fn shadow_wrapper_transform_moves_assigned_boxes_with_their_painted_descendants() {
    let page = slotted_page(
        r#"
        <div id=wrapper style='display:block;width:200px;height:60px;transform:translate(40px,25px)'>
            <slot name=content style='display:contents'>
                <div id=fallback style='display:block;width:300px;height:300px'></div>
            </slot>
        </div>
    "#,
    );
    assert_boxes_match_paint(&page, 40.0, 25.0, 60.0);
}

#[test]
fn reverse_column_alignment_moves_slotted_descendants_with_the_flex_item() {
    let page = slotted_page(
        r#"
        <main style='display:flex;flex-direction:column-reverse;width:200px;height:100px'>
            <section style='display:block;width:100px;height:20px'>
                <slot name=content style='display:contents'>
                    <div id=fallback style='display:block;width:300px;height:300px'></div>
                </slot>
            </section>
        </main>
    "#,
    );
    assert_boxes_match_paint(&page, 0.0, 80.0, 100.0);
}
