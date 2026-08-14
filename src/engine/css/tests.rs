use super::*;
use crate::engine::dom;

#[test]
fn composites_translucent_css_colors_source_over() {
    assert_eq!(
        Color {
            red: 0,
            green: 0,
            blue: 0,
            alpha: 38,
        }
        .composite_over(Color::WHITE),
        Color::rgb(217, 217, 217)
    );
}

#[test]
fn cascades_specificity_and_inline_styles() {
    let dom = dom::parse(
        r#"<style>p { color:red } .note {color:blue} #main {font-size:20px}</style>
               <p id="main" class="note" style="color:#123456">hello</p>"#,
    );
    let styles = StyleSet::from_dom(&dom, &[], 1000.0);
    let paragraph = dom.elements_named("p").next().unwrap();
    let style = styles.get(&paragraph);
    assert_eq!(style.color, Color::rgb(0x12, 0x34, 0x56));
    assert_eq!(style.font_size, 20.0);
}

#[test]
fn important_author_rules_beat_normal_inline_styles() {
    let dom = dom::parse(
        r#"<style>
                .outer { opacity: 0.5 !important; font-size: 18px !important; line-height: 2em; }
               </style><p class="outer" style="opacity: 1; font-size: 36px">hello</p>"#,
    );
    let styles = StyleSet::from_dom(&dom, &[], 1000.0);
    let paragraph = dom.elements_named("p").next().unwrap();
    let style = styles.get(&paragraph);

    assert_eq!(style.opacity, 0.5);
    assert_eq!(style.font_size, 18.0);
    assert_eq!(style.line_height, 36.0);
}

#[test]
fn resolves_author_relative_font_sizes_against_the_parent() {
    let dom = dom::parse(
        r#"<style>
                body { font-size: 20px }
                h2 { font-size: 1.31em }
                h3 { font: bold 125%/1.4 Arial }
               </style><h2>result title</h2><h3>shorthand title</h3>"#,
    );
    let styles = StyleSet::from_dom(&dom, &[], 1000.0);
    let heading = dom.elements_named("h2").next().unwrap();
    let shorthand = dom.elements_named("h3").next().unwrap();
    assert!((styles.get(&heading).font_size - 26.2).abs() < 0.01);
    assert!((styles.get(&shorthand).font_size - 25.0).abs() < 0.01);
    assert!((styles.get(&shorthand).line_height - 35.0).abs() < 0.01);
}

#[test]
fn resolves_background_images_against_the_stylesheet_url() {
    let dom = dom::parse(r#"<a class="logo"></a>"#);
    let stylesheets = vec![(
            "https://cdn.example/assets/css/site.css".to_string(),
            r#".logo {
                width: 65px;
                height: 60px;
                background: no-repeat center/auto 36px url('../logo.svg'), linear-gradient(transparent, transparent);
            }"#
                .to_string(),
        )];
    let styles = StyleSet::from_sources(&dom, "https://example.com/page/", &stylesheets, 1000.0);
    let logo = dom.elements_named("a").next().unwrap();
    let style = styles.get(&logo);
    assert_eq!(
        style.background_image.as_deref(),
        Some("https://cdn.example/assets/logo.svg")
    );
    assert!(!style.background_repeat_x);
    assert!(!style.background_repeat_y);
    assert_eq!(style.background_position_x, Length::Percent(50.0));
    assert_eq!(style.background_position_y, Length::Percent(50.0));
    assert_eq!(
        style.background_size,
        BackgroundSize::Explicit {
            width: Length::Auto,
            height: Length::Px(36.0)
        }
    );
}

#[test]
fn matches_descendants_children_compounds_and_not() {
    let dom = dom::parse(
        r#"<style>
                #app > .row a.link { color: rgb(1,2,3); }
                .row:not(.hidden) { background-color: #abcdef; }
               </style><div id="app"><div class="row"><a class="link">x</a></div></div>"#,
    );
    let styles = StyleSet::from_dom(&dom, &[], 1000.0);
    let link = dom.elements_named("a").next().unwrap();
    let row = dom
        .elements_named("div")
        .find(|node| node.has_class("row"))
        .unwrap();
    assert_eq!(styles.get(&link).color, Color::rgb(1, 2, 3));
    assert_eq!(
        styles.get(&row).background_color,
        Color::rgb(0xab, 0xcd, 0xef)
    );
}

#[test]
fn matches_functional_selector_lists_and_root_conservatively() {
    let dom = dom::parse(
        r#"<style>
                :root { background-color: #010203; }
                :is(#links, #ads) .result { color: #123456; }
                p:not(.muted, .hidden) { background-color: #abcdef; }
                .outside:has(.result) { color: red; }
               </style>
               <main id="links"><p class="result">shown</p></main>
               <p class="muted">muted</p><div class="outside"><span class="result">x</span></div>"#,
    );
    let styles = StyleSet::from_dom(&dom, &[], 1000.0);
    let html = dom.elements_named("html").next().unwrap();
    let result = dom
        .elements_named("p")
        .find(|node| node.has_class("result"))
        .unwrap();
    let muted = dom
        .elements_named("p")
        .find(|node| node.has_class("muted"))
        .unwrap();
    let outside = dom
        .elements_named("div")
        .find(|node| node.has_class("outside"))
        .unwrap();
    assert_eq!(styles.get(&html).background_color, Color::rgb(1, 2, 3));
    assert_eq!(
        styles.get(&result).background_color,
        Color::rgb(0xab, 0xcd, 0xef)
    );
    assert_eq!(styles.get(&result).color, Color::rgb(0x12, 0x34, 0x56));
    assert_eq!(styles.get(&muted).background_color, Color::TRANSPARENT);
    assert_eq!(styles.get(&outside).color, Color::BLACK);
}

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
fn matches_attribute_selectors_instead_of_treating_them_as_wildcards() {
    let dom = dom::parse(
        r#"<style>
                .item[data-display="block"] { display: block; color: green; }
                .item[data-display="none"] { display: none; color: red; }
                [data-tags~="featured"] { background-color: #123456; }
               </style>
               <div class="item" data-display="block" data-tags="home featured">visible</div>
               <div class="item" data-display="none">hidden</div>"#,
    );
    let styles = StyleSet::from_dom(&dom, &[], 1000.0);
    let mut items = dom
        .elements_named("div")
        .filter(|node| node.has_class("item"));
    let visible = items.next().unwrap();
    let hidden = items.next().unwrap();
    assert_eq!(styles.get(&visible).display, Display::Block);
    assert_eq!(styles.get(&visible).color, Color::rgb(0, 128, 0));
    assert_eq!(
        styles.get(&visible).background_color,
        Color::rgb(0x12, 0x34, 0x56)
    );
    assert_eq!(styles.get(&hidden).display, Display::None);
    assert_eq!(styles.get(&hidden).color, Color::rgb(255, 0, 0));
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
fn applies_html_rendering_states_for_details_and_dialog() {
    let dom = dom::parse(
        r#"<details id="closed"><summary id="closed-summary">More</summary><p id="closed-content">Hidden</p></details>
               <details open><summary>Less</summary><p id="open-content">Visible</p></details>
               <dialog id="closed-dialog">Closed</dialog>
               <dialog id="open-dialog" open>Open</dialog>"#,
    );
    let styles = StyleSet::from_dom(&dom, &[], 1000.0);
    let by_id = |id: &str| {
        dom::Node::descendants(&dom.document)
            .find(|node| node.attr("id").as_deref() == Some(id))
            .unwrap()
    };

    assert_eq!(styles.get(&by_id("closed-summary")).display, Display::Block);
    assert_eq!(styles.get(&by_id("closed-content")).display, Display::None);
    assert_eq!(styles.get(&by_id("open-content")).display, Display::Block);
    assert_eq!(styles.get(&by_id("closed-dialog")).display, Display::None);
    assert_eq!(styles.get(&by_id("open-dialog")).display, Display::Block);
}

#[test]
fn honors_explicit_inheritance_after_user_agent_defaults() {
    let dom = dom::parse(
        r#"<style>
                html { box-sizing: border-box; }
                * { box-sizing: inherit; }
                .field { width: 80px; max-width: 90px; background-color: #212121; }
                input { width: inherit; max-width: inherit; background-color: inherit; }
               </style><div class="field"><input></div>"#,
    );
    let styles = StyleSet::from_dom(&dom, &[], 1000.0);
    let input = dom.elements_named("input").next().unwrap();
    let style = styles.get(&input);
    assert_eq!(style.box_sizing, BoxSizing::BorderBox);
    assert_eq!(style.width, Length::Px(80.0));
    assert_eq!(style.max_width, Length::Px(90.0));
    assert_eq!(style.background_color, Color::rgb(0x21, 0x21, 0x21));
}

#[test]
fn cascades_inherited_custom_properties_before_var_substitution() {
    let dom = dom::parse(
        r#"<style>
                :root { --max-content-width: 590px; --Accent: rgb(1, 2, 3); }
                .wide { --max-content-width: 672px; }
                .target {
                    max-width: calc(var(--max-content-width) - 72px);
                    width: var(--missing-width, 80px);
                    color: var(--Accent);
                }
                .cycle { --a: var(--b); --b: var(--a); width: var(--a, 44px); }
               </style>
               <div class="wide"><p class="target">result</p></div>
               <p class="cycle">fallback</p>"#,
    );
    let styles = StyleSet::from_dom(&dom, &[], 1000.0);
    let target = dom
        .elements_named("p")
        .find(|node| node.has_class("target"))
        .unwrap();
    let cycle = dom
        .elements_named("p")
        .find(|node| node.has_class("cycle"))
        .unwrap();
    assert_eq!(
        styles
            .get(&target)
            .custom_properties
            .get("--max-content-width")
            .map(String::as_str),
        Some("672px")
    );
    assert_eq!(
        substitute_variables(
            "calc(var(--max-content-width) - 72px)",
            &styles.get(&target).custom_properties,
        )
        .as_deref(),
        Some("calc(672px - 72px)")
    );
    assert_eq!(styles.get(&target).max_width, Length::Px(600.0));
    assert_eq!(styles.get(&target).width, Length::Px(80.0));
    assert_eq!(styles.get(&target).color, Color::rgb(1, 2, 3));
    assert_eq!(styles.get(&cycle).width, Length::Px(44.0));
}

#[test]
fn parses_css_lengths_and_colors() {
    assert_eq!(parse_length("50%"), Some(Length::Percent(50.0)));
    assert_eq!(parse_length("1.5em"), Some(Length::Em(1.5)));
    assert_eq!(parse_length("calc(672px - 72px)"), Some(Length::Px(600.0)));
    assert_eq!(
        parse_length("calc(100% - 20px)").and_then(|length| length.resolve(200.0, 16.0)),
        Some(180.0)
    );
    assert_eq!(
        parse_length("calc((2 * 10px) + 1em)").and_then(|length| length.resolve(200.0, 16.0)),
        Some(36.0)
    );
    assert_eq!(
        parse_color("rgba(10, 20, 30, .5)"),
        Some(Color {
            red: 10,
            green: 20,
            blue: 30,
            alpha: 128,
        })
    );
}
