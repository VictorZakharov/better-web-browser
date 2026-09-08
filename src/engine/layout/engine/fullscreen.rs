use super::*;
use crate::engine::invalidation::{MutationKind, RenderInvalidation};

struct FixedMeasurer;

impl TextMeasurer for FixedMeasurer {
    fn measure(&mut self, text: &str, font: &FontSpec) -> (f32, f32) {
        (text.chars().count() as f32 * font.size * 0.5, font.size)
    }
}

#[test]
fn hidden_assigned_slot_does_not_suppress_a_fullscreen_top_layer() {
    let page = Page::parse(
        "<x-host><main slot=content><p>fullscreen content</p></main></x-host><p id=outside>normal page</p>",
        "https://example.test/",
    );
    let host = page.dom.elements_named("x-host").next().unwrap();
    let target = page.dom.elements_named("main").next().unwrap();
    let outside = page.dom.elements_named("p").nth(1).unwrap();
    let shadow = Node::attach_shadow(
        &host,
        crate::engine::dom::ShadowRootMode::Open,
        false,
        false,
        false,
    )
    .unwrap();
    Node::replace_inner_html(
        &shadow,
        "<slot name=content style='display:none'></slot>",
        true,
    );
    let mut sparse_page = page.layout_snapshot();
    let mut invalidation = RenderInvalidation::full(page.dom.document.id());
    for fullscreen in [true, false, true] {
        target.set_fullscreen(fullscreen);
        let stats = sparse_page.refresh_layout_styles_after_invalidation_for_viewport(
            800.0,
            600.0,
            &invalidation,
        );
        let dense = layout_page(&page, 800.0, 600.0, &mut FixedMeasurer);
        let sparse = layout_page(&sparse_page, 800.0, 600.0, &mut FixedMeasurer);
        assert_eq!(sparse.node_bounds, dense.node_bounds);
        assert_eq!(sparse.content_height, dense.content_height);
        assert!(
            stats.layout_changed,
            "entering/exiting the deferred top layer requires layout"
        );
        assert_eq!(sparse.node_bounds.contains_key(&target.id()), fullscreen);
        assert_eq!(sparse.node_bounds.contains_key(&outside.id()), !fullscreen);
        if fullscreen {
            let rect = sparse.node_bounds[&target.id()];
            assert_eq!((rect.width, rect.height), (800.0, 600.0));
        }
        invalidation = RenderInvalidation {
            roots: vec![host.id()],
            impact: MutationKind::State.impact(),
            mutation_count: 1,
            ..RenderInvalidation::default()
        };
    }
    // A real shadow-including hidden ancestor still suppresses the top layer.
    host.set_attr("style", "display:none");
    sparse_page.refresh_layout_styles_after_invalidation_for_viewport(800.0, 600.0, &invalidation);
    let dense = layout_page(&page, 800.0, 600.0, &mut FixedMeasurer);
    let sparse = layout_page(&sparse_page, 800.0, 600.0, &mut FixedMeasurer);
    assert_eq!(sparse.node_bounds, dense.node_bounds);
    assert!(!sparse.node_bounds.contains_key(&target.id()));
}

#[test]
fn unassigned_fullscreen_subtree_matches_dense_layout_after_mutation_and_exit() {
    let page = Page::parse(
        "<x-host><section style='font-size:12px'><main><p style='margin:0'>fullscreen content</p></main></section></x-host><p id=outside>normal page</p>",
        "https://example.test/",
    );
    let host = page.dom.elements_named("x-host").next().unwrap();
    let inherited = page.dom.elements_named("section").next().unwrap();
    let target = page.dom.elements_named("main").next().unwrap();
    let child = page.dom.elements_named("p").next().unwrap();
    let outside = page.dom.elements_named("p").nth(1).unwrap();
    let shadow = Node::attach_shadow(
        &host,
        crate::engine::dom::ShadowRootMode::Open,
        false,
        false,
        false,
    )
    .unwrap();
    Node::replace_inner_html(&shadow, "<div>shadow content without a slot</div>", true);
    let mut sparse_page = page.layout_snapshot();
    let mut dense_page = page.layout_snapshot();
    let full = RenderInvalidation::full(page.dom.document.id());
    sparse_page.refresh_layout_styles_after_invalidation_for_viewport(800.0, 600.0, &full);
    dense_page.refresh_resources_after_invalidation_for_viewport(800.0, 600.0, &full);
    let invalidation = RenderInvalidation {
        // Host invalidation deliberately cannot reach the unassigned light-DOM subtree
        // through the normal composed traversal, including already-cached ancestors.
        roots: vec![host.id()],
        impact: MutationKind::State.impact(),
        mutation_count: 1,
        ..RenderInvalidation::default()
    };
    let mut initial_child_x = 0.0;
    for step in 0..6 {
        match step {
            0 => target.set_fullscreen(true),
            1 => {
                target.set_attr("style", "padding:24px");
            }
            2 => {
                inherited.set_attr("style", "font-size:32px");
            }
            3 => {
                inherited.set_attr("style", "font-size:32px;display:none");
            }
            4 => {
                inherited.set_attr("style", "font-size:32px;display:block");
            }
            _ => target.set_fullscreen(false),
        }
        let sparse_stats = sparse_page.refresh_layout_styles_after_invalidation_for_viewport(
            800.0,
            600.0,
            &invalidation,
        );
        let dense_stats = dense_page.refresh_resources_after_invalidation_for_viewport(
            800.0,
            600.0,
            &invalidation,
        );
        let fresh = layout_page(&page, 800.0, 600.0, &mut FixedMeasurer);
        let dense = layout_page(&dense_page, 800.0, 600.0, &mut FixedMeasurer);
        let sparse = layout_page(&sparse_page, 800.0, 600.0, &mut FixedMeasurer);
        assert_eq!(sparse.node_bounds, fresh.node_bounds, "sparse step {step}");
        assert_eq!(dense.node_bounds, fresh.node_bounds, "dense step {step}");
        assert!(
            sparse_stats.layout_changed && dense_stats.layout_changed,
            "step {step}"
        );
        let visible = !matches!(step, 3 | 5);
        assert_eq!(
            sparse.node_bounds.contains_key(&target.id()),
            visible,
            "step {step}"
        );
        assert_eq!(
            sparse.node_bounds.contains_key(&outside.id()),
            !visible,
            "step {step}"
        );
        if !visible {
            continue;
        }
        let rect = sparse.node_bounds[&target.id()];
        assert_eq!((rect.width, rect.height), (800.0, 600.0));
        if step == 0 {
            initial_child_x = sparse.node_bounds[&child.id()].x;
        } else {
            assert!(sparse.node_bounds[&child.id()].x > initial_child_x);
        }
        let styles = sparse_page.cached_style_for_viewport(800.0, 600.0).unwrap();
        assert_eq!(
            styles.get(&child).font_size,
            if step >= 2 { 32.0 } else { 12.0 }
        );
    }
}
