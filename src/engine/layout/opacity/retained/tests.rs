use super::*;
use crate::engine::invalidation::MutationKind;
use crate::engine::layout::test_support::FixedMeasurer;

fn page(extra: &str, content: &str) -> Page {
    let mut page = Page::parse(
        &format!(
            "<!doctype html><style>body{{margin:0}}#outer{{opacity:.8;overflow:hidden;\
             width:240px;height:160px;transform:translate(10px,5px)}}\
             #inner{{display:block;opacity:.6;background:red}}{extra}</style>\
             <main id=outer><span id=inner>text<b> nested</b></span>{content}</main>\
             <aside>unrelated sibling</aside>"
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

fn refresh(page: &mut Page, changes: &[(&str, &str)]) -> (RenderInvalidation, StyleRefreshStats) {
    let mut roots = Vec::new();
    for (id, style) in changes {
        let target = node(page, id);
        target.set_attr("style", style);
        roots.push(target.id());
    }
    let invalidation = RenderInvalidation {
        roots,
        impact: MutationKind::Attribute("style").impact(),
        mutation_count: changes.len(),
        ..Default::default()
    };
    let stats =
        page.refresh_layout_styles_after_invalidation_for_viewport(800., 600., &invalidation);
    (invalidation, stats)
}

fn refresh_retained(
    retained: &mut LayoutOutput,
    page: &Page,
    invalidation: &RenderInvalidation,
    stats: &StyleRefreshStats,
) -> bool {
    retained.try_refresh_opacity(page, [800., 600., 800.], invalidation, stats)
}

#[test]
fn nested_opacity_updates_match_full_paint_and_keep_geometry_and_hits() {
    let mut page = page(
        "#positioned{position:absolute;z-index:-1;background:blue;width:80px;height:40px}",
        "<div id=positioned>negative layer</div>",
    );
    let mut retained = layout_page(&page, 800., 600., &mut FixedMeasurer);
    let bounds = retained.node_bounds.clone();
    let fragments = retained.fragments.clone();
    let order = retained.node_paint_order.clone();
    for opacity in ["opacity:.4", "opacity:0", "opacity:.7"] {
        let (invalidation, stats) = refresh(&mut page, &[("outer", opacity), ("inner", opacity)]);
        assert!(stats.opacity_only && stats.changed_styles == 2);
        assert!(refresh_retained(
            &mut retained,
            &page,
            &invalidation,
            &stats
        ));
        let full = layout_page(&page, 800., 600., &mut FixedMeasurer);
        assert_eq!(retained.items, full.items);
        assert_eq!(retained.node_bounds, bounds);
        assert_eq!(retained.resize_boxes, full.resize_boxes);
        assert_eq!(retained.clip_paths, full.clip_paths);
        assert_eq!(retained.node_paint_order, order);
        assert_eq!(retained.hit_excluded, full.hit_excluded);
        assert!(std::sync::Arc::ptr_eq(&retained.fragments, &fragments));
        let retained_hits = HitTestSnapshot::from_layout(&retained, &page.dom.document);
        let full_hits = HitTestSnapshot::from_layout(&full, &page.dom.document);
        for (x, y) in [(12., 8.), (80., 40.), (260., 180.)] {
            let ids = |nodes: Vec<NodeRef>| nodes.iter().map(|n| n.id()).collect::<Vec<_>>();
            assert_eq!(
                ids(retained_hits.elements_at(x, y)),
                ids(full_hits.elements_at(x, y))
            );
        }
    }
}

#[test]
fn mixed_styles_and_stacking_context_changes_fall_back_without_partial_updates() {
    for style in [
        "opacity:1",
        "opacity:.4;width:100px",
        "opacity:.4;color:blue",
        "opacity:.4;z-index:5",
        "opacity:.4;pointer-events:none",
        "opacity:.4;visibility:hidden",
        "opacity:.4;transform:scale(.5)",
    ] {
        let mut page = page("", "");
        let mut retained = layout_page(&page, 800., 600., &mut FixedMeasurer);
        let before = retained.items.clone();
        let (invalidation, stats) =
            refresh(&mut page, &[("outer", "opacity:.3"), ("inner", style)]);
        assert!(
            !refresh_retained(&mut retained, &page, &invalidation, &stats),
            "{style}"
        );
        assert_eq!(retained.items, before, "{style}");
    }
}

#[test]
fn changed_inline_or_hidden_owners_cannot_hide_behind_a_supported_group() {
    for extra in ["#inner{display:inline}", "#inner{display:none}"] {
        let mut page = page(extra, "");
        let mut retained = layout_page(&page, 800., 600., &mut FixedMeasurer);
        let before = retained.items.clone();
        let (invalidation, stats) = refresh(
            &mut page,
            &[("outer", "opacity:.3"), ("inner", "opacity:.4")],
        );
        assert!(stats.opacity_only);
        assert!(!refresh_retained(
            &mut retained,
            &page,
            &invalidation,
            &stats
        ));
        assert_eq!(retained.items, before);
    }
}

#[test]
fn changed_generated_paint_invalidates_the_opacity_proof() {
    let mut page = page(
        "#inner::before{content:'before'}#inner[style]::before{color:blue}",
        "",
    );
    let mut retained = layout_page(&page, 800., 600., &mut FixedMeasurer);
    let before = retained.items.clone();
    let (invalidation, stats) = refresh(&mut page, &[("inner", "opacity:.4")]);
    assert!(!stats.opacity_only);
    assert!(!refresh_retained(
        &mut retained,
        &page,
        &invalidation,
        &stats
    ));
    assert_eq!(retained.items, before);
}

#[test]
fn unchanged_generated_opacity_stays_inside_its_owning_group() {
    let mut page = page(
        "#inner::before{display:block;content:'before';opacity:.2}",
        "",
    );
    let mut retained = layout_page(&page, 800., 600., &mut FixedMeasurer);
    let (invalidation, stats) = refresh(&mut page, &[("inner", "opacity:.4")]);
    assert!(refresh_retained(
        &mut retained,
        &page,
        &invalidation,
        &stats
    ));
    assert_eq!(
        retained.items,
        layout_page(&page, 800., 600., &mut FixedMeasurer).items
    );
}

#[test]
fn unknown_content_and_stylesheet_work_cannot_reuse_opacity_paint() {
    let mut page = page("", "");
    let mut retained = layout_page(&page, 800., 600., &mut FixedMeasurer);
    let before = retained.items.clone();
    let (mut invalidation, stats) = refresh(&mut page, &[("inner", "opacity:.4")]);
    invalidation.impact = invalidation.impact.union(MutationKind::ChildList.impact());
    assert!(!refresh_retained(
        &mut retained,
        &page,
        &invalidation,
        &stats
    ));
    invalidation.impact = MutationKind::Attribute("style").impact();
    invalidation.rebuild_style_rules = true;
    assert!(!refresh_retained(
        &mut retained,
        &page,
        &invalidation,
        &stats
    ));
    assert_eq!(retained.items, before);
}
