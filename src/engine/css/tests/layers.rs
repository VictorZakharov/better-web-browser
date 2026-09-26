//! CSS Cascade 5 layer ordering across stylesheets and conditional groups.
use super::*;

fn color(css: &str) -> Color {
    let html = format!("<style>{css}</style><p id=subject class=target>x</p>");
    let dom = dom::parse(&html);
    let styles = StyleSet::from_dom(&dom, &[], 800.0);
    styles.get(&dom.elements_named("p").next().unwrap()).color
}

#[test]
fn later_normal_layers_win_even_over_higher_specificity() {
    assert_eq!(
        color(
            "@layer base, theme; @layer theme { p {color:blue} } @layer base { #subject {color:red} }"
        ),
        Color::rgb(0, 0, 255),
    );
    assert_eq!(
        color("@layer base { #subject {color:red} } p {color:green}"),
        Color::rgb(0, 128, 0),
    );
}

#[test]
fn important_layers_reverse_order_and_unlayered_important_is_weakest() {
    assert_eq!(
        color(
            "@layer base, theme; @layer theme { #subject {color:blue!important} } @layer base { p {color:red!important} } p {color:green!important}"
        ),
        Color::rgb(255, 0, 0),
    );
}

#[test]
fn nested_layer_parent_rules_follow_their_child_for_normal_declarations() {
    assert_eq!(
        color("@layer app { p {color:green} @layer theme { #subject {color:red} } }"),
        Color::rgb(0, 128, 0),
    );
    assert_eq!(
        color(
            "@layer app { p {color:green!important} @layer theme { #subject {color:red!important} } }"
        ),
        Color::rgb(255, 0, 0),
    );
}

#[test]
fn false_conditions_do_not_declare_their_layers() {
    assert_eq!(
        color(
            "@media print { @layer theme { p {color:red} } } @layer base {p {color:blue}} @layer theme {p {color:green}}"
        ),
        Color::rgb(0, 128, 0),
    );
}

#[test]
fn revert_layer_restores_the_value_before_the_current_layer() {
    assert_eq!(
        color("@layer base {p {color:red}} @layer theme {p {color:blue} p {color:revert-layer}}"),
        Color::rgb(255, 0, 0),
    );
    assert_eq!(
        color("@layer base {p {color:red}} p {color:revert-layer}"),
        Color::rgb(255, 0, 0),
    );
}

#[test]
fn important_revert_layer_ignores_intervening_normal_layers() {
    assert_eq!(
        color(
            "@layer base, theme; @layer base {p {color:red}} @layer theme {p {color:blue; color:revert-layer!important}}"
        ),
        Color::rgb(255, 0, 0),
    );
    assert_eq!(
        color("@layer base {p {color:red}} p {color:blue; color:revert-layer!important}"),
        Color::rgb(255, 0, 0),
    );
    assert_eq!(
        color("@layer base {p {color:red}} p {color:blue; color:REVERT-LAYER!important}"),
        Color::rgb(255, 0, 0),
    );
    assert_eq!(
        color(
            "@layer base, theme; @layer base {p {color:red!important}} @layer theme {p {color:blue!important}} @layer base {p {color:revert-layer!important}}"
        ),
        Color::rgb(0, 0, 255),
    );
}

#[test]
fn custom_properties_use_the_same_layer_rollback_order() {
    assert_eq!(
        color(
            "@layer base, theme; @layer base {p {--tone:red}} @layer theme {p {--tone:blue; --tone:revert-layer}} p {color:var(--tone)}"
        ),
        Color::rgb(255, 0, 0),
    );
    assert_eq!(
        color(
            "@layer base, theme; @layer base {p {--tone:red}} @layer theme {p {--tone:blue; --tone:revert-layer!important}} p {color:var(--tone)}"
        ),
        Color::rgb(255, 0, 0),
    );
    assert_eq!(
        color(
            "@layer base, theme; @layer theme {p {--tone:blue!important}} @layer base {p {--tone:revert-layer!important}} p {color:var(--tone)}"
        ),
        Color::rgb(0, 0, 255),
    );
}

#[test]
fn anonymous_layers_are_distinct_and_named_layers_can_reopen() {
    assert_eq!(
        color("@layer {#subject {color:red}} @layer {p {color:blue}}"),
        Color::rgb(0, 0, 255),
    );
    assert_eq!(
        color(
            "@layer base {p {color:red}} @layer theme {p {color:blue}} @layer base {#subject {color:green}}"
        ),
        Color::rgb(0, 0, 255),
    );
}

fn imported_color(css: &str, sources: &[StylesheetSource]) -> Color {
    let html = format!("<style>{css}</style><p id=subject>x</p>");
    let dom = dom::parse(&html);
    let styles = StyleSet::from_sources_for_media_environment(
        &dom,
        "https://example.test/page",
        sources,
        media::MediaEnvironment::new(800.0, 600.0, 1.0, false),
    );
    styles.get(&dom.elements_named("p").next().unwrap()).color
}

#[test]
fn named_import_layer_uses_early_statement_order() {
    let source = StylesheetSource::linked(
        "https://example.test/theme.css",
        "#subject {color:blue}".into(),
    );
    assert_eq!(
        imported_color(
            "@layer reset, theme; @import url(https://example.test/theme.css) layer(theme); @layer reset { #subject {color:red} }",
            &[source],
        ),
        Color::rgb(0, 0, 255),
    );
}

#[test]
fn imported_layer_declaration_survives_a_failed_fetch() {
    assert_eq!(
        imported_color(
            "@import url(https://example.test/missing.css) layer(base); @layer theme {p {color:blue}} @layer base {#subject {color:red}}",
            &[],
        ),
        Color::rgb(0, 0, 255),
    );
}

#[test]
fn anonymous_import_layers_are_separate_and_keep_nested_rules_inside() {
    let first = StylesheetSource::linked(
        "https://example.test/first.css",
        "#subject {color:red}".into(),
    );
    let second =
        StylesheetSource::linked("https://example.test/second.css", "p {color:blue}".into());
    assert_eq!(
        imported_color(
            "@import url(https://example.test/first.css) layer; @import url(https://example.test/second.css) layer;",
            &[first, second],
        ),
        Color::rgb(0, 0, 255),
    );
}

#[test]
fn nested_import_layer_names_are_relative_to_the_parent_import() {
    let parent = StylesheetSource::linked(
        "https://example.test/parent.css",
        "@import url(https://example.test/child.css) layer(theme); p {font-size:18px}".into(),
    );
    let child = StylesheetSource::linked("https://example.test/child.css", "p {color:blue}".into());
    assert_eq!(
        imported_color(
            "@import url(https://example.test/parent.css) layer(framework); @layer framework.other {p {color:green}}",
            &[parent, child],
        ),
        Color::rgb(0, 128, 0),
    );
}
