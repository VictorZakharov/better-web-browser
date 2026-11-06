//! `:hover` designation changes are bounded by the rules that use `:hover`.
use super::*;
use crate::engine::invalidation::{HOVER_STATE, record_attribute_change};

/// Moves the pointer onto the element with id `to` (or off the page), refreshes only the recorded
/// designation changes, and asserts every element matches a from-scratch cascade.
fn hover(
    html: &str,
    to: Option<&str>,
    path: &mut Vec<crate::engine::dom::NodeRef>,
    page: &mut Page,
) -> StyleRefreshStats {
    let target = to.map(|id| {
        Node::descendants(&page.dom.document)
            .find(|node| node.attr("id").as_deref() == Some(id))
            .expect("hover target")
    });
    let boundary = Node::update_hover_path(path, target);
    let mut invalidation = RenderInvalidation {
        impact: MutationKind::PointerDesignation.impact(),
        ..RenderInvalidation::default()
    };
    for node in boundary.leaving.iter().chain(&boundary.entering) {
        record_attribute_change(&mut invalidation.attribute_changes, node, HOVER_STATE);
    }
    let stats = page.refresh_resources_after_invalidation(800.0, &invalidation);
    let fresh = page.style(800.0);
    let incremental = page.cached_style(800.0).expect("refreshed styles");
    for node in Node::descendants(&page.dom.document).filter(|node| node.element().is_some()) {
        assert!(
            incremental.styles.get(&node.id()) == fresh.styles.get(&node.id()),
            "stale style for <{}> hovering {to:?} in {html}",
            node.tag_name().unwrap_or_default(),
        );
    }
    stats
}

fn document(css: &str) -> String {
    let rows = "<section><p>text <b>bold</b></p><p class=foo>foo</p></section>".repeat(20);
    format!(
        "<html><style>{css}</style><body><main>{rows}<nav class=menu id=menu>\
         <a id=link href=#>link</a><span class=item>item</span></nav><p id=after>after</p>\
         </main></body></html>"
    )
}

#[test]
fn entering_the_page_restyles_hover_subjects_not_the_document() {
    // `<html>` and `<body>` enter `:hover` with every target. A sibling rule whose compound they
    // cannot match (`.menu`) must not widen their change to the parent scope (the document).
    for (css, target) in [
        ("a:hover{color:red}", "link"),
        (".menu:hover .item{display:block}", "link"),
        ("html:hover .foo{color:blue}", "link"),
        ("a:not(:hover){text-decoration:none}", "link"),
        (".menu:hover + p{color:red}", "after"),
        ("nav:hover ~ p{color:red}", "after"),
    ] {
        let html = document(css);
        let mut page = Page::parse(&html, "https://example.test/");
        page.refresh_resources(800.0);
        let mut path = Vec::new();
        let entered = hover(&html, Some(target), &mut path, &mut page);
        assert!(!entered.full_rebuild, "{css}");
        assert!(
            entered.recomputed_styles < entered.total_styles / 3,
            "{css}: {entered:?}"
        );
        let left = hover(&html, None, &mut path, &mut page);
        assert!(
            left.recomputed_styles < left.total_styles / 3,
            "{css}: {left:?}"
        );
    }
}

#[test]
fn sibling_and_relational_hover_selectors_stay_correct() {
    for css in [
        ".menu:hover + p{color:red}",
        ".menu:hover ~ p{color:red}",
        "main:has(a:hover) .foo{color:green}",
        "nav:has(> :hover) .item{display:none}",
        ":hover{outline:1px solid}",
    ] {
        let html = document(css);
        let mut page = Page::parse(&html, "https://example.test/");
        page.refresh_resources(800.0);
        let mut path = Vec::new();
        hover(&html, Some("link"), &mut path, &mut page);
        hover(&html, Some("after"), &mut path, &mut page);
        hover(&html, None, &mut path, &mut page);
    }
}
