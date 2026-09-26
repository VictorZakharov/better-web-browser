use super::super::*;

#[test]
fn applies_media_width_queries() {
    let dom =
        dom::parse(r#"<style>@media (max-width: 600px) { body { color: green } }</style><p>x</p>"#);
    let narrow = StyleSet::from_dom(&dom, &[], 500.0);
    let wide = StyleSet::from_dom(&dom, &[], 900.0);
    let body = dom.elements_named("body").next().unwrap();
    assert_eq!(narrow.get(&body).color, Color::rgb(0, 128, 0));
    assert_eq!(wide.get(&body).color, Color::BLACK);
}

#[test]
fn applies_calculated_media_breakpoints_to_responsive_sidebar_rules() {
    let dom = dom::parse(
        r#"<style>
            .client-js .pinned { display: none }
            @media screen and (max-width: calc(1120px - 1px)) {
                .client-js .pinned { display: none }
            }
            @media screen and (min-width: 1120px) {
                .client-js.feature-pinned .column .pinned { display: block }
            }
        </style>
        <html class="client-js feature-pinned">
            <body><aside class="column"><nav class="pinned">Contents</nav></aside></body>
        </html>"#,
    );
    let pinned = dom.elements_named("nav").next().unwrap();

    assert_eq!(
        StyleSet::from_dom(&dom, &[], 1118.0).get(&pinned).display,
        Display::None
    );
    assert_eq!(
        StyleSet::from_dom(&dom, &[], 1868.0).get(&pinned).display,
        Display::Block
    );
}

#[test]
fn evaluates_css_supports_against_implemented_property_values() {
    let dom = dom::parse(
        r#"<style>
            .grid { display: none }
            @supports (display: grid) { .grid { display: block } }
            @supports (position: sticky) { .sticky { display: none } }
            @supports (display: grid) and (position: sticky) { .compound { display: none } }
            @supports not (position: sticky) { .negated { display: none } }
        </style>
        <div class="grid"></div><div class="sticky"></div>
        <div class="compound"></div><div class="negated"></div>"#,
    );
    let styles = StyleSet::from_dom(&dom, &[], 1200.0);
    let display = |class| {
        let node = dom
            .elements_named("div")
            .find(|node| node.has_class(class))
            .unwrap();
        styles.get(&node).display
    };

    assert_eq!(display("grid"), Display::Block);
    assert_eq!(display("sticky"), Display::None);
    assert_eq!(display("compound"), Display::None);
    assert_eq!(display("negated"), Display::Block);
}

#[test]
fn rejects_vendor_media_queries_for_other_engines() {
    let dom = dom::parse(
        r#"<style>
                body { color: green; }
                @media screen and (-ms-high-contrast: active),
                       screen and (-ms-high-contrast: none) {
                    body { color: red; }
                }
               </style><p>x</p>"#,
    );
    let styles = StyleSet::from_dom(&dom, &[], 1000.0);
    let body = dom.elements_named("body").next().unwrap();
    assert_eq!(styles.get(&body).color, Color::rgb(0, 128, 0));
}

#[test]
fn media_range_and_group_conditions_control_stylesheet_rules() {
    let dom = dom::parse(
        r#"<style>
            .ranged, .grouped, .outside { display: none }
            @media (500px < width <= 1000px) and (orientation: landscape) {
                .ranged { display: block }
            }
            @media ((width > 700px) or (height > 700px)) {
                .grouped { display: block }
            }
            @media (width > 1200px) { .outside { display: block } }
        </style><div class="ranged"></div><div class="grouped"></div>
        <div class="outside"></div>"#,
    );
    let display = |width: f32, height: f32, class: &str| {
        let node = dom
            .elements_named("div")
            .find(|node| node.has_class(class))
            .unwrap();
        StyleSet::from_sources_for_viewport(&dom, "https://example.test/", &[], width, height)
            .get(&node)
            .display
    };
    assert_eq!(display(800.0, 600.0, "ranged"), Display::Block);
    assert_eq!(display(800.0, 600.0, "grouped"), Display::Block);
    assert_eq!(display(800.0, 600.0, "outside"), Display::None);
    assert_eq!(display(500.0, 800.0, "ranged"), Display::None);
    assert_eq!(display(500.0, 800.0, "grouped"), Display::Block);
    assert_eq!(display(1300.0, 700.0, "outside"), Display::Block);
}

#[test]
fn invalid_conditional_syntax_cannot_enable_a_stylesheet_branch() {
    let dom = dom::parse(
        r#"<style>
            .valid, .malformed-media, .malformed-supports, .negated-malformed { display: none }
            @media (width >= 600px), :: { .valid { display: block } }
            @media (width >= 600px) trailing { .malformed-media { display: block } }
            @supports (display: grid) trailing { .malformed-supports { display: block } }
            @supports not (display: grid) trailing { .negated-malformed { display: block } }
        </style><div class="valid"></div><div class="malformed-media"></div>
        <div class="malformed-supports"></div><div class="negated-malformed"></div>"#,
    );
    let styles = StyleSet::from_dom(&dom, &[], 800.0);
    let display = |class| {
        let node = dom
            .elements_named("div")
            .find(|node| node.has_class(class))
            .unwrap();
        styles.get(&node).display
    };
    assert_eq!(display("valid"), Display::Block);
    assert_eq!(display("malformed-media"), Display::None);
    assert_eq!(display("malformed-supports"), Display::None);
    assert_eq!(display("negated-malformed"), Display::None);
}

#[test]
fn supports_conditions_combine_declarations_and_supported_selectors() {
    let dom = dom::parse(
        r#"<style>
            .compound, .selector, .unknown-selector { display: none }
            @supports ((display: grid) and (position: sticky)) or (display: flex) {
                .compound { display: block }
            }
            @supports selector(.card > .title) { .selector { display: block } }
            @supports selector(:totally-unknown-pseudo) {
                .unknown-selector { display: block }
            }
        </style><div class="compound"></div><div class="selector"></div>
        <div class="unknown-selector"></div>"#,
    );
    let styles = StyleSet::from_dom(&dom, &[], 800.0);
    let display = |class| {
        let node = dom
            .elements_named("div")
            .find(|node| node.has_class(class))
            .unwrap();
        styles.get(&node).display
    };
    assert_eq!(display("compound"), Display::Block);
    assert_eq!(display("selector"), Display::Block);
    assert_eq!(display("unknown-selector"), Display::None);
}
