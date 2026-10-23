//! Attributes only selectors read are bounded like class tokens.
use super::*;

/// Sets `name` on `#target`, refreshes only the attribute-change roots, and asserts every element
/// matches a from-scratch cascade of the final document.
fn change_attribute(html: &str, name: &str, value: Option<&str>) -> StyleRefreshStats {
    let mut page = Page::parse(html, "https://example.test/");
    page.refresh_resources(800.0);
    let target = Node::descendants(&page.dom.document)
        .find(|node| node.attr("id").as_deref() == Some("target"))
        .expect("#target");
    let changed = match value {
        Some(value) => target.set_attr(name, value),
        None => target.remove_attr(name),
    };
    assert!(changed, "{name} must change");
    let invalidation = RenderInvalidation {
        impact: MutationKind::Attribute("data-x").impact(),
        mutation_count: 1,
        attribute_changes: vec![AttributeChange {
            node: target.id(),
            tokens: Vec::new(),
            attributes: vec![name.to_ascii_lowercase()],
        }],
        ..RenderInvalidation::default()
    };
    let stats = page.refresh_resources_after_invalidation(800.0, &invalidation);
    let fresh = page.style(800.0);
    let incremental = page.cached_style(800.0).expect("refreshed styles");
    for node in Node::descendants(&page.dom.document).filter(|node| node.element().is_some()) {
        assert!(
            incremental.styles.get(&node.id()) == fresh.styles.get(&node.id()),
            "stale style for <{}> after {name}={value:?} in {html}",
            node.tag_name().unwrap_or_default(),
        );
    }
    stats
}

fn page(css: &str) -> String {
    let rows = "<section><p>text <b>bold</b></p><h2>heading</h2></section>".repeat(20);
    format!(
        "<style>{css}</style><main id=target data-state=closed>{rows}</main><aside><p>aside</p></aside>"
    )
}

#[test]
fn unread_attributes_restyle_only_the_element() {
    for css in [
        "p{color:red}",
        "[data-other] p{color:red}",
        "[role=tab]{color:red}",
    ] {
        for (name, value) in [
            ("aria-busy", Some("true")),
            ("data-state", None),
            ("role", Some("main")),
        ] {
            let stats = change_attribute(&page(css), name, value);
            assert!(!stats.full_rebuild, "{css} {name}");
            assert!(stats.recomputed_styles <= 3, "{css} {name}: {stats:?}");
        }
    }
}

#[test]
fn ancestor_attribute_selectors_restyle_matching_descendants() {
    for css in [
        "[data-state=open] h2{color:red}",
        "main[data-state^=op] > section h2{color:red}",
        ":is([data-state=open]) h2::before{content:'x'}",
    ] {
        let html = page(css);
        let stats = change_attribute(&html, "data-state", Some("open"));
        assert!(!stats.full_rebuild, "{css}");
        assert!(
            stats.recomputed_styles < stats.total_styles / 2,
            "{css}: {stats:?}"
        );
    }
}

#[test]
fn inherited_changes_from_an_attribute_restyle_the_subtree() {
    let html = page("[data-state=open]{color:red;font-size:30px}");
    change_attribute(&html, "data-state", Some("open"));
}

#[test]
fn sibling_and_relational_attribute_selectors_stay_correct() {
    for (css, html) in [
        (
            "[aria-expanded=true] + p{color:red}",
            "<main><div id=target></div><p>x</p></main>",
        ),
        (
            "main:has([data-x]) p{color:red}",
            "<main><div id=target></div><p>x</p></main>",
        ),
        (
            "p:not([data-x] *){color:red}",
            "<main id=target><p>x</p></main>",
        ),
    ] {
        let name = if css.contains("aria") {
            "aria-expanded"
        } else {
            "data-x"
        };
        let value = if name == "aria-expanded" { "true" } else { "" };
        change_attribute(&format!("<style>{css}</style>{html}"), name, Some(value));
    }
}
