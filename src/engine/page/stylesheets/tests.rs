use super::*;

#[test]
fn discovery_is_bounded_and_rejects_excess_dependencies_explicitly() {
    let imports = (0..100)
        .map(|n| format!("@import '{n}.css';"))
        .collect::<String>();
    let mut page = Page::parse(
        &format!("<style>{imports}</style>"),
        "https://example.test/",
    );
    assert_eq!(page.resources.len(), crate::limits::MAX_STYLESHEETS);
    assert_eq!(page.diagnostics.len(), 1);
    page.discover_stylesheet_dependencies();
    assert_eq!(page.resources.len(), crate::limits::MAX_STYLESHEETS);
    assert_eq!(page.diagnostics.len(), 1);
}

#[test]
fn changed_media_and_replaced_parent_sources_discover_new_dependencies() {
    let mut page = Page::parse(
        "<link rel=stylesheet href=root.css>",
        "https://example.test/",
    );
    page.add_linked_stylesheet(
        "https://example.test/root.css",
        "@import 'wide.css' (min-width: 1500px);".into(),
    );
    assert_eq!(page.resources.len(), 1);
    page.set_media_environment(MediaEnvironment::new(1600.0, 900.0, 1.0, false));
    page.discover_stylesheet_dependencies();
    assert_eq!(page.resources.len(), 2);
    page.add_linked_stylesheet(
        "https://example.test/root.css",
        "@import 'replacement.css';".into(),
    );
    assert_eq!(page.resources.len(), 3);
    assert_eq!(page.stylesheet_sources.len(), 1);
}

#[test]
fn redirect_response_base_is_used_for_descendant_imports() {
    let mut page = Page::parse(
        "<link rel=stylesheet href=root.css>",
        "https://example.test/",
    );
    page.add_linked_stylesheet_response(
        "https://example.test/root.css",
        "https://cdn.test/styles/root.css",
        "@import 'child.css';".into(),
    );
    assert!(page.resources.contains(&PageResource::Stylesheet {
        url: "https://cdn.test/styles/child.css".into()
    }));
}
