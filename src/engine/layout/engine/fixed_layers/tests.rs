use super::*;
use crate::engine::invalidation::{MutationKind, RenderInvalidation};
use crate::engine::layout::test_support::FixedMeasurer;
mod benchmark;
mod fallbacks;
mod multiple;
mod work;

fn page(extra: &str) -> Page {
    let mut page = Page::parse(
        &format!(
            r#"<!doctype html><style>
        body{{margin:0;overflow:hidden}} .background{{height:900px}}
        #overlay{{position:fixed;inset:0;z-index:20;display:flex;align-items:center;
            justify-content:center;background:#333;color:white}}
        #title{{width:280px;height:100px;background:#c00;transform:translate(5px,6px)}}
        #title::before{{content:'before'}} #bar{{width:240px;height:8px;
            transform-origin:left;transform:scaleX(.1)}} {extra}
        </style><main class=background>unrelated <b>normal flow</b></main>
        <section id=overlay><article id=title>title <i>text fragment</i>
        <div id=bar></div></article></section><aside>other sibling</aside>"#
        ),
        "https://example.test/",
    );
    page.refresh_resources_for_viewport(800., 600.);
    page
}

fn node(page: &Page, id: &str) -> NodeRef {
    Node::composed_descendants(&page.dom.document)
        .find(|node| node.attr("id").as_deref() == Some(id))
        .unwrap()
}

fn refresh(page: &mut Page, id: &str, style: &str) -> RenderInvalidation {
    let node = node(page, id);
    node.set_attr("style", style);
    let invalidation = RenderInvalidation {
        roots: vec![node.id()],
        impact: MutationKind::Attribute("style").impact(),
        mutation_count: 1,
        ..Default::default()
    };
    page.refresh_layout_styles_after_invalidation_for_viewport(800., 600., &invalidation);
    invalidation
}

fn assert_same(retained: &LayoutOutput, full: &LayoutOutput, page: &Page) {
    assert_eq!(retained.items, full.items);
    assert_eq!(retained.node_bounds, full.node_bounds);
    assert_eq!(retained.resize_boxes, full.resize_boxes);
    assert_eq!(retained.clip_paths, full.clip_paths);
    assert_eq!(retained.hit_excluded, full.hit_excluded);
    assert_eq!(retained.node_paint_order, full.node_paint_order);
    assert_eq!(retained.content_height, full.content_height);
    assert_eq!(retained.background, full.background);
    for node in Node::composed_descendants(&page.dom.document) {
        assert_eq!(
            retained.fragments.layout_rect(node.id()),
            full.fragments.layout_rect(node.id())
        );
        assert_eq!(
            retained.fragments.element(node.id()),
            full.fragments.element(node.id())
        );
        assert_eq!(
            retained.fragments.range_text(node.id(), 0, 100),
            full.fragments.range_text(node.id(), 0, 100)
        );
        for point in [(0., 0.), (100., 80.), (400., 300.), (700., 590.)] {
            assert_eq!(
                retained.point_in_transformed_box(&node, point.0, point.1),
                full.point_in_transformed_box(&node, point.0, point.1)
            );
        }
    }
}

#[test]
fn fixed_overlay_reflow_matches_full_paint_and_geometry() {
    let mut page = page("");
    let mut retained = layout_page(&page, 800., 600., &mut FixedMeasurer);
    assert_eq!(retained.fixed_layers.layers.len(), 1);
    for (id, style) in [
        ("bar", "transform:scaleX(.45)"),
        (
            "title",
            "transform:translate(60px,-40px) rotate(5deg);clip-path:inset(20% 0 0 0)",
        ),
        ("title", "width:340px;height:180px;padding:9px;opacity:.7"),
        ("title", "visibility:hidden;pointer-events:none"),
        ("title", "display:none"),
        ("title", "transform:scale(.8);color:yellow"),
    ] {
        let invalidation = refresh(&mut page, id, style);
        assert!(
            retained.try_refresh_fixed_layers(
                &page,
                [800., 600., 800.],
                &invalidation,
                &mut FixedMeasurer
            ),
            "{id}: {style}"
        );
        let full = layout_page(&page, 800., 600., &mut FixedMeasurer);
        assert_same(&retained, &full, &page);
    }
}

#[test]
fn fixed_root_size_insets_and_effects_can_change_without_moving_siblings() {
    let mut page = page("");
    let mut retained = layout_page(&page, 800., 600., &mut FixedMeasurer);
    for style in [
        "inset:20px 50px;width:auto;height:auto;opacity:.5",
        "inset:auto;left:12px;top:17px;width:300px;height:200px;transform:scale(.9)",
    ] {
        let invalidation = refresh(&mut page, "overlay", style);
        assert!(retained.try_refresh_fixed_layers(
            &page,
            [800., 600., 800.],
            &invalidation,
            &mut FixedMeasurer
        ));
        assert_same(
            &retained,
            &layout_page(&page, 800., 600., &mut FixedMeasurer),
            &page,
        );
    }
}

#[test]
fn fallbacks_leave_retained_output_unchanged() {
    for style in ["position:static", "inset:auto", "z-index:21"] {
        let mut page = page("");
        let mut retained = layout_page(&page, 800., 600., &mut FixedMeasurer);
        let before = retained.clone();
        let invalidation = refresh(&mut page, "overlay", style);
        assert!(!retained.try_refresh_fixed_layers(
            &page,
            [800., 600., 800.],
            &invalidation,
            &mut FixedMeasurer
        ));
        assert_eq!(retained.items, before.items);
        assert_eq!(retained.node_bounds, before.node_bounds);
    }
}
