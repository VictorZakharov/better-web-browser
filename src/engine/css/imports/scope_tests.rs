use super::*;
use crate::engine::css::{Color, StyleSet, StylesheetSource};
use crate::engine::dom;

fn source(name: &str, css: &str) -> StylesheetSource {
    StylesheetSource::linked(&format!("https://example.test/{name}"), css.into())
}

fn colors(html: &str, sources: &[StylesheetSource], ids: &[&str]) -> Vec<Color> {
    let document = dom::parse(html);
    let styles = StyleSet::from_sources_for_media_environment(
        &document,
        "https://example.test/page",
        sources,
        MediaEnvironment::new(800.0, 600.0, 1.0, false),
    );
    ids.iter()
        .map(|id| {
            let node = document
                .elements_named("p")
                .find(|node| node.attr("id").as_deref() == Some(id))
                .unwrap();
            styles.get(&node).color
        })
        .collect()
}

#[test]
fn import_modifiers_allow_any_order_once_and_validate_scope() {
    let examples = [
        "@import 'a.css' layer(theme) scope(.card) supports(display: block) screen;",
        "@import 'a.css' scope(.card) supports(display: block) layer(theme) screen;",
        "@import 'a.css' supports(display: block) layer(theme) scope(.card) screen;",
    ];
    for example in examples {
        let imports = parse(example);
        assert_eq!(imports.len(), 1, "{example}");
        let import = &imports[0];
        assert_eq!(import.layer.as_deref(), Some("theme"));
        assert_eq!(import.scope.as_deref(), Some(".card"));
        assert_eq!(import.supports.as_deref(), Some("display: block"));
        assert_eq!(import.media, "screen");
        assert!(import.matches(MediaEnvironment::new(800.0, 600.0, 1.0, false)));
    }
    assert_eq!(
        parse("@import 'a.css' scope;")[0].scope.as_deref(),
        Some("")
    );
    assert_eq!(
        parse("@import 'a.css' scope();")[0].scope.as_deref(),
        Some("")
    );
    assert_eq!(
        parse("@import 'a.css' scope((.card) to (.stop));")[0]
            .scope
            .as_deref(),
        Some("(.card) to (.stop)")
    );
    for invalid in [
        "@import 'a.css' scope(.card::before);",
        "@import 'a.css' scope(.card) scope(.other);",
        "@import 'a.css' layer(base) layer(other);",
        "@import 'a.css' supports(display: block) supports(display: grid);",
        "@import 'a.css' supports();",
        "@import 'a.css' supports((display: block) and);",
        "@import 'a.css' supports((display: block) or (display: grid) and (color: red));",
    ] {
        assert!(parse(invalid).is_empty(), "{invalid}");
    }
    let unknown = parse("@import 'a.css' supports(future-property: future-value);");
    assert_eq!(
        unknown.len(),
        1,
        "unknown declarations remain valid imports"
    );
    assert!(!unknown[0].matches(MediaEnvironment::new(800.0, 600.0, 1.0, false)));
}

#[test]
fn imported_scope_filters_subject_without_reinterpreting_top_level_selectors() {
    let linked = source("theme.css", "body p { color: red }");
    let html = "<style>@import 'theme.css' scope(.card);</style>\
        <p id=root class=card>root</p>\
        <div class=card><p id=inside>inside</p></div>\
        <p id=outside>outside</p>";
    assert_eq!(
        colors(html, &[linked], &["root", "inside", "outside"]),
        [
            Color::rgb(255, 0, 0),
            Color::rgb(255, 0, 0),
            Color::rgb(0, 0, 0)
        ]
    );
}

#[test]
fn imported_top_level_scope_and_ampersand_anchor_the_scope_root() {
    let linked = source("theme.css", ":scope { color: red } & > p { color: blue }");
    let html = "<style>@import 'theme.css' scope(.card);</style>\
        <section class=card id=root><p id=child>child</p></section>\
        <section id=outside><p id=other>other</p></section>";
    let document = dom::parse(html);
    let styles = StyleSet::from_sources_for_media_environment(
        &document,
        "https://example.test/page",
        &[linked],
        MediaEnvironment::new(800.0, 600.0, 1.0, false),
    );
    let root = document
        .elements_named("section")
        .find(|node| node.attr("id").as_deref() == Some("root"))
        .unwrap();
    assert_eq!(styles.get(&root).color, Color::rgb(255, 0, 0));
    assert_eq!(
        styles
            .get(&document.elements_named("p").next().unwrap())
            .color,
        Color::rgb(0, 0, 255)
    );
}

#[test]
fn bare_import_scope_uses_owner_parent_as_implicit_root() {
    let linked = source("theme.css", "p { color: red }");
    for modifier in ["scope", "scope()"] {
        let html = format!(
            "<div><style>@import 'theme.css' {modifier};</style>\
             <p id=inside>inside</p></div><p id=outside>outside</p>"
        );
        assert_eq!(
            colors(&html, std::slice::from_ref(&linked), &["inside", "outside"]),
            [Color::rgb(255, 0, 0), Color::rgb(0, 0, 0)]
        );
    }
}

#[test]
fn repeated_imports_of_same_url_keep_independent_scope_occurrences() {
    let linked = source("theme.css", "p { color: red }");
    let html = "<style>@import 'theme.css' scope(.first);\
        @import 'theme.css' scope(.second);</style>\
        <div class=first><p id=first>first</p></div>\
        <div class=second><p id=second>second</p></div>\
        <p id=outside>outside</p>";
    assert_eq!(
        colors(html, &[linked], &["first", "second", "outside"]),
        [
            Color::rgb(255, 0, 0),
            Color::rgb(255, 0, 0),
            Color::rgb(0, 0, 0)
        ]
    );
}

#[test]
fn import_scope_limits_exclude_the_limit_and_its_descendants() {
    let linked = source("theme.css", "p { color: red }");
    let html = "<style>@import 'theme.css' scope((.card) to (.stop));</style>\
        <div class=card><p id=inside>inside</p>\
        <div class=stop><p id=at-limit>at limit</p></div>\
        <p id=after-limit>after</p></div>";
    assert_eq!(
        colors(html, &[linked], &["inside", "at-limit", "after-limit"]),
        [
            Color::rgb(255, 0, 0),
            Color::rgb(0, 0, 0),
            Color::rgb(255, 0, 0)
        ]
    );
}

#[test]
fn imported_scope_applies_inside_group_rules_but_not_to_global_layer_definition() {
    let linked = source(
        "theme.css",
        "@media screen { @layer theme { p { color: red } } }",
    );
    let html = "<style>@import 'theme.css' scope(.card);</style>\
        <div class=card><p id=inside>inside</p></div>\
        <p id=outside>outside</p>";
    assert_eq!(
        colors(html, &[linked], &["inside", "outside"]),
        [Color::rgb(255, 0, 0), Color::rgb(0, 0, 0)]
    );
}

#[test]
fn nested_import_scopes_intersect_and_obey_media_supports_and_layer() {
    let parent = source("parent.css", "@import 'child.css' scope(.inner); ");
    let child = source("child.css", "p { color: red }");
    let html = "<style>@import 'parent.css' supports(display: block)\
        scope(.outer) layer(theme) screen;</style>\
        <div class=outer><div class=inner><p id=both>both</p></div>\
        <p id=outer-only>outer</p></div>\
        <div class=inner><p id=inner-only>inner</p></div>";
    assert_eq!(
        colors(
            html,
            &[parent, child],
            &["both", "outer-only", "inner-only"]
        ),
        [
            Color::rgb(255, 0, 0),
            Color::rgb(0, 0, 0),
            Color::rgb(0, 0, 0)
        ]
    );
    let blocked = source("theme.css", "p { color: red }");
    let print_html = "<style>@import 'theme.css' scope(.outer) print;</style>\
        <div class=outer><p id=blocked>blocked</p></div>";
    assert_eq!(
        colors(print_html, &[blocked], &["blocked"]),
        [Color::rgb(0, 0, 0)]
    );
}
