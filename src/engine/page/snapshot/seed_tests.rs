//! The script layout snapshot starts from the page's styles only while they describe the
//! document exactly, and then owns an independent copy.
use super::*;
use crate::engine::css::{PseudoElement, StyleRefreshStats};
use crate::engine::dom::Node;
use crate::engine::invalidation::{
    AttributeChange, MutationKind, RenderInvalidation, changed_class_tokens,
};

const PAGE: &str = "<style>.on{color:red} .on::before{content:'on'} p::after{content:attr(data-x)}\
    </style><main><p id=target data-x=a>text</p><p data-x=b>other</p></main>";

fn styled_page() -> Page {
    let mut page = Page::parse(PAGE, "https://example.com/");
    page.refresh_resources(800.0);
    page
}

fn target(page: &Page) -> NodeRef {
    Node::descendants(&page.dom.document)
        .find(|node| node.attr("id").as_deref() == Some("target"))
        .expect("#target")
}

/// Adds `on` to `#target`, flushes the snapshot with that change, and asserts every style and
/// generated box equals a fresh cascade of the document.
fn flush_class_change(snapshot: &mut Page) -> StyleRefreshStats {
    let node = target(snapshot);
    node.set_attr("class", "on");
    let invalidation = RenderInvalidation {
        impact: MutationKind::Attribute("class").impact(),
        mutation_count: 1,
        attribute_changes: vec![AttributeChange {
            node: node.id(),
            tokens: changed_class_tokens("", "on").expect("tokens changed"),
            attributes: Vec::new(),
        }],
        ..RenderInvalidation::default()
    };
    let stats =
        snapshot.refresh_layout_styles_after_invalidation_for_viewport(800.0, 800.0, &invalidation);
    let fresh = snapshot.style(800.0);
    let flushed = snapshot.cached_style(800.0).expect("flushed styles");
    // A snapshot built from scratch skips styles below `display: none`.
    for paragraph in snapshot.dom.elements_named("p") {
        assert!(flushed.styles.contains_key(&paragraph.id()));
    }
    for node in Node::descendants(&snapshot.dom.document).filter(|node| node.element().is_some()) {
        if let Some(style) = flushed.styles.get(&node.id()) {
            assert!(fresh.styles.get(&node.id()) == Some(style));
        }
        for pseudo in [PseudoElement::Before, PseudoElement::After] {
            assert_eq!(
                flushed
                    .generated_pseudo(&node, pseudo)
                    .map(|node| node.text_content()),
                fresh
                    .generated_pseudo(&node, pseudo)
                    .map(|node| node.text_content()),
            );
        }
    }
    stats
}

#[test]
fn a_current_page_seeds_the_snapshot_with_an_independent_copy() {
    let page = styled_page();
    let mut snapshot = page.layout_snapshot();
    let stats = flush_class_change(&mut snapshot);
    assert!(!stats.full_rebuild, "{stats:?}");
    assert!(stats.recomputed_styles < stats.total_styles, "{stats:?}");

    // The page keeps its own styles and generated boxes.
    let node = target(&page);
    let page_styles = page.cached_style(800.0).expect("page styles");
    let snapshot_styles = snapshot.cached_style(800.0).expect("snapshot styles");
    assert!(
        page_styles
            .generated_pseudo(&node, PseudoElement::Before)
            .is_none()
    );
    assert!(
        snapshot_styles
            .generated_pseudo(&node, PseudoElement::Before)
            .is_some()
    );
    let page_after = page_styles.generated_pseudo(&node, PseudoElement::After);
    let snapshot_after = snapshot_styles.generated_pseudo(&node, PseudoElement::After);
    assert!(!std::rc::Rc::ptr_eq(
        &page_after.unwrap(),
        &snapshot_after.unwrap()
    ));
}

#[test]
fn a_page_behind_the_document_or_with_deferred_work_does_not_seed() {
    // A mutation after the page's last refresh is not in the seed or the flush's invalidation.
    let page = styled_page();
    target(&page).set_attr("data-x", "changed");
    let mut snapshot = page.layout_snapshot();
    assert!(flush_class_change(&mut snapshot).full_rebuild);

    let mut page = styled_page();
    let root = page.dom.document.id();
    page.defer_style_invalidation(800.0, 800.0, &RenderInvalidation::full(root));
    let mut snapshot = page.layout_snapshot();
    assert!(flush_class_change(&mut snapshot).full_rebuild);
}

#[test]
fn a_rule_rebuild_without_dom_roots_refreshes_a_seed_incrementally() {
    // Script reports a stylesheet source change as a rule rebuild with no DOM root.
    let mut page = styled_page();
    let mut snapshot = page.layout_snapshot();
    page.stylesheet_sources = vec![crate::engine::css::StylesheetSource::injected(
        "https://example.com/late.css",
        "p{margin-left:3px}".into(),
    )];
    page.refresh_resources(800.0);
    snapshot.synchronize_layout_snapshot(&page);
    let stats = snapshot.refresh_layout_styles_after_invalidation_for_viewport(
        800.0,
        800.0,
        &RenderInvalidation {
            impact: MutationKind::Stylesheet.impact(),
            mutation_count: 1,
            rebuild_style_rules: true,
            ..RenderInvalidation::default()
        },
    );
    assert!(!stats.full_rebuild, "{stats:?}");
    // Geometry published before the seed may predate it, so adopting one lays out again.
    assert!(stats.layout_changed, "{stats:?}");
    assert!(!flush_class_change(&mut snapshot).full_rebuild);
}

#[test]
fn synchronizing_reseeds_a_snapshot_whose_styles_were_dropped() {
    let mut page = styled_page();
    let mut snapshot = page.layout_snapshot();
    page.stylesheet_sources = vec![crate::engine::css::StylesheetSource::injected(
        "https://example.com/late.css",
        "p{margin-left:3px}".into(),
    )];
    page.refresh_resources(800.0);
    snapshot.synchronize_layout_snapshot(&page);
    assert!(snapshot.cached_styles.is_none());
    assert!(!flush_class_change(&mut snapshot).full_rebuild);
}
