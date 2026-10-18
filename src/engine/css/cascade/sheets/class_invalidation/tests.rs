use crate::engine::css::StyleRefreshStats;
use crate::engine::dom::Node;
use crate::engine::invalidation::{
    ClassChange, MutationKind, RenderInvalidation, changed_class_tokens,
};
use crate::engine::page::Page;

/// Changes `#target`'s class, refreshes only the class-change roots, and asserts every element
/// matches a from-scratch cascade of the final document.
fn change_class(html: &str, class: &str) -> StyleRefreshStats {
    let mut page = Page::parse(html, "https://example.test/");
    page.refresh_resources(800.0);
    let target = Node::descendants(&page.dom.document)
        .find(|node| node.attr("id").as_deref() == Some("target"))
        .expect("#target");
    let previous = target.attr("class").unwrap_or_default();
    target.set_attr("class", class);
    let invalidation = RenderInvalidation {
        impact: MutationKind::Attribute("class").impact(),
        mutation_count: 1,
        class_changes: vec![ClassChange {
            node: target.id(),
            tokens: changed_class_tokens(&previous, class).expect("tokens changed"),
        }],
        ..RenderInvalidation::default()
    };
    let stats = page.refresh_resources_after_invalidation(800.0, &invalidation);
    let fresh = page.style(800.0);
    let incremental = page.cached_style(800.0).expect("refreshed styles");
    for node in Node::descendants(&page.dom.document).filter(|node| node.element().is_some()) {
        assert!(
            incremental.styles.get(&node.id()) == fresh.styles.get(&node.id()),
            "stale style for <{}> after class {previous:?} -> {class:?} in {html}",
            node.tag_name().unwrap_or_default(),
        );
    }
    stats
}

fn page(css: &str, root_class: &str) -> String {
    let rows = "<section><p>text <b>bold</b></p><p class=foo>foo</p></section>".repeat(20);
    format!(
        "<html id=target class='{root_class}'><style>{css}</style><body><main>{rows}</main>\
         <aside class=foo><span>aside</span></aside></body></html>"
    )
}

#[test]
fn root_class_changes_restyle_matching_descendants_not_the_document() {
    for css in [
        ".client-js .foo{color:red}",
        ".client-js > body .foo{color:red}",
        ".client-js aside span{color:red}",
        "html:not(.client-js) .foo{display:none}",
        ".client-js .foo::before{content:'x'}",
    ] {
        let html = page(css, "client-nojs");
        let stats = change_class(&html, "client-js");
        assert!(!stats.full_rebuild, "{css}");
        assert!(
            stats.recomputed_styles < stats.total_styles / 3,
            "{css}: {stats:?}"
        );
    }
}

#[test]
fn unreferenced_and_subject_only_tokens_stop_at_an_unchanged_element() {
    for css in [".foo{color:red}", ".open{outline:1px solid}"] {
        let html = page(css, "closed");
        let stats = change_class(&html, "open");
        assert!(stats.recomputed_styles <= 3, "{css}: {stats:?}");
    }
}

#[test]
fn inherited_changes_restyle_the_subtree() {
    for css in [
        ".big{font-size:30px}",
        ".dark{color:white;--accent:red} p{border-color:var(--accent)}",
    ] {
        let html = page(css, "");
        change_class(
            &html,
            if css.starts_with(".big") {
                "big"
            } else {
                "dark"
            },
        );
    }
}

#[test]
fn sibling_relational_nested_and_attribute_selectors_keep_the_parent_scope() {
    for (css, html) in [
        (
            ".a + p{color:red}",
            "<main><div id=target></div><p>x</p></main>",
        ),
        (
            ".a ~ p b{color:red}",
            "<main><div id=target></div><p><b>x</b></p></main>",
        ),
        (
            "main:has(.a){color:red}",
            "<main><div id=target></div><p>x</p></main>",
        ),
        (
            "p:nth-child(1 of .a){color:red}",
            "<main><p id=target></p><p>x</p></main>",
        ),
        (
            ":is(.a p) b{color:red}",
            "<main id=target><p><b>x</b></p></main>",
        ),
        ("[class^=a] p{color:red}", "<main id=target><p>x</p></main>"),
        (".a *{color:red}", "<main id=target><p><b>x</b></p></main>"),
    ] {
        change_class(&format!("<style>{css}</style>{html}"), "a");
    }
}

#[test]
fn quirks_mode_class_tokens_match_case_insensitively() {
    let html = "<style>.ACTIVE .item{color:red}</style><main id=target><p class=item>x</p></main>";
    change_class(html, "active");
}
