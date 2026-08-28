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
    assert_eq!(display("sticky"), Display::Block);
    assert_eq!(display("compound"), Display::Block);
    assert_eq!(display("negated"), Display::None);
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
