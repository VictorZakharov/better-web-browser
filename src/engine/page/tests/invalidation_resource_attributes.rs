use super::super::*;
use crate::engine::css::Color;
use crate::engine::invalidation::{MutationKind, RenderInvalidation};

fn source_change(css: &str) -> (StyleRefreshStats, StyleRefreshStats, Color) {
    let mut page = Page::parse(
        &format!(
            "<style>{css}</style><main><div id=branch><img id=picture></div></main><aside>neighbor</aside>"
        ),
        "https://example.com/",
    );
    let initial = page.refresh_resources(800.0);
    let picture = page.dom.elements_named("img").next().unwrap();
    let branch = picture.parent().unwrap();
    let neighbor = page.dom.elements_named("aside").next().unwrap();
    picture.set_attr("src", "picture.png");
    let refreshed = page.refresh_resources_after_invalidation(
        800.0,
        &RenderInvalidation {
            roots: vec![branch.id()],
            impact: MutationKind::Attribute("src").impact(),
            mutation_count: 1,
            ..Default::default()
        },
    );
    let neighbor_color = page.cached_style(800.0).unwrap().get(&neighbor).color;
    (initial, refreshed, neighbor_color)
}

#[test]
fn image_source_change_keeps_unrelated_relational_rules_local() {
    let (initial, refreshed, _) = source_change("main:has(.active) + aside { color: red }");
    assert!(!refreshed.full_rebuild);
    assert!(refreshed.recomputed_styles < initial.total_styles);
}

#[test]
fn image_source_change_refreshes_relational_attribute_dependents() {
    let (initial, refreshed, color) = source_change("main:has(img[src]) + aside { color: red }");
    assert_eq!(color, Color::rgb(255, 0, 0));
    // The flipped `main` anchor refreshes its parent's subtree, which holds the `aside`
    // sibling dependent, rather than the whole document.
    assert!(!refreshed.full_rebuild);
    assert!(refreshed.recomputed_styles < initial.total_styles);
}
