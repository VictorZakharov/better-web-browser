//! The script layout snapshot's geometry passes replay the layouts its page retained.
use super::*;
use crate::engine::invalidation::{
    AttributeChange, MutationKind, RenderInvalidation, changed_class_tokens,
};
use crate::engine::layout::layout_geometry_with_style_viewport;

fn geometry(page: &Page, measurer: &mut Counting) -> LayoutOutput {
    layout_geometry_with_style_viewport(page, WIDTH, HEIGHT, WIDTH, measurer)
}

/// Sets `class` on `#id` and brings `snapshot`'s styles up to date with just that change, as a
/// script geometry read does.
fn change_class(snapshot: &mut Page, id: &str, class: &str) {
    let node = element(snapshot, id);
    let previous = node.attr("class").unwrap_or_default();
    node.set_attr("class", class);
    let invalidation = RenderInvalidation {
        impact: MutationKind::Attribute("class").impact(),
        mutation_count: 1,
        attribute_changes: vec![AttributeChange {
            node: node.id(),
            tokens: changed_class_tokens(&previous, class).expect("tokens changed"),
            attributes: Vec::new(),
        }],
        ..RenderInvalidation::default()
    };
    snapshot.refresh_layout_styles_after_invalidation_for_viewport(WIDTH, HEIGHT, &invalidation);
}

/// Geometry of a snapshot with nothing to replay, from styles computed from scratch.
fn cold_geometry(page: &Page) -> LayoutOutput {
    let mut cold = page.layout_snapshot();
    cold.layout_reuse.borrow_mut().source = Default::default();
    cold.layout_box_tree = Default::default();
    cold.refresh_layout_styles_after_invalidation_for_viewport(
        WIDTH,
        HEIGHT,
        &RenderInvalidation::full(page.dom.document.id()),
    );
    geometry(&cold, &mut Counting::default())
}

#[test]
fn a_script_layout_replays_the_pages_layouts_of_unchanged_subtrees() {
    let page = page();
    settle(&page);
    let mut snapshot = page.layout_snapshot();
    change_class(&mut snapshot, "list", "tall");
    snapshot.take_layout_stats();
    let mut measurer = Counting::default();
    let warm = geometry(&snapshot, &mut measurer);
    let stats = snapshot.take_layout_stats();
    assert!(stats.subtrees_replayed >= 5, "{stats:?}");
    assert_eq!(stats.items_replayed, 0, "geometry copies no display items");
    assert!(warm.items.is_empty() && warm.node_paint_order.is_empty());
    assert_same_output(&warm, &cold_geometry(&page));

    // The page's own results stay valid for its next painted pass.
    let replayed = page.layout_reuse.borrow().subtrees.len();
    assert!(replayed > 0);
    page.take_layout_stats();
    layout(&page, &mut Counting::default());
    assert!(page.take_layout_stats().subtrees_replayed >= 5);
}

#[test]
fn generated_content_is_laid_out_from_the_snapshots_own_boxes() {
    let mut page = Page::parse(
        "<style>section{display:flow-root} .on::before{content:'generated words '}</style>\
         <main><section id=a><p id=marked>text</p></section><section id=b><p>other</p></section></main>",
        "https://example.test/",
    );
    page.refresh_resources_for_viewport(WIDTH, HEIGHT);
    element(&page, "marked").set_attr("class", "on");
    page.refresh_resources_for_viewport(WIDTH, HEIGHT);
    settle(&page);
    let mut snapshot = page.layout_snapshot();
    change_class(&mut snapshot, "b", "changed");
    let warm = geometry(&snapshot, &mut Counting::default());
    assert_same_output(&warm, &cold_geometry(&page));
}
