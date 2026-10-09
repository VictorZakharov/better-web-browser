use super::*;

fn requested(html: &str, css: &str) -> Vec<(String, String, u16)> {
    let mut page = Page::parse(html, "https://example.test/");
    page.add_stylesheet_from("https://example.test/css/fonts.css", css.into());
    page.refresh_resources_for_viewport(800.0, 600.0);
    page.resources
        .iter()
        .filter_map(|resource| match resource {
            PageResource::Font {
                url,
                unicode_range,
                weight,
                ..
            } => Some((url.clone(), unicode_range.clone(), *weight)),
            _ => None,
        })
        .collect()
}

const SUBSETS: &str = r#"
 @font-face {font-family:Subset;src:url(upper.woff);unicode-range:U+41-5A}
 @font-face {font-family:Subset;src:url(lower.woff);unicode-range:U+61-7A}
 @font-face {font-family:Subset;src:url(digits.woff);unicode-range:U+30-39}
 body,input,textarea,p::before {font-family:Subset}
"#;

#[test]
fn font_usage_does_not_hydrate_a_sparse_display_none_subtree() {
    let mut page = Page::parse(
        "<body><section style='display:none'><p>ABC</p></section><p>xyz</p></body>",
        "https://example.test/",
    );
    let sources = [crate::engine::css::StylesheetSource::injected(
        "https://example.test/css/fonts.css",
        SUBSETS.into(),
    )];
    let mut styles = StyleSet::from_sources_for_layout(
        &page.dom,
        "https://example.test/",
        &sources,
        MediaEnvironment::new(800., 600., 1., false),
    );
    let hidden = page.dom.elements_named("p").next().unwrap();
    let count = styles.styles.len();
    assert!(!styles.styles.contains_key(&hidden.id()));
    page.request_visible_fonts(&mut styles);
    assert_eq!(styles.styles.len(), count);
    assert!(!styles.styles.contains_key(&hidden.id()));
    let fonts = page
        .resources
        .iter()
        .filter_map(|resource| match resource {
            PageResource::Font { url, .. } => Some(url.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(fonts, ["https://example.test/css/lower.woff"]);
}

#[test]
fn only_intersecting_equal_style_subsets_are_requested() {
    let fonts = requested("<body><p>ABxy</p></body>", SUBSETS);
    assert_eq!(fonts.len(), 2);
    assert_eq!(fonts[0].0, "https://example.test/css/upper.woff");
    assert_eq!(fonts[0].1, "U+41-5A");
    assert_eq!(fonts[1].0, "https://example.test/css/lower.woff");
    assert_eq!(fonts[1].1, "U+61-7A");
}

#[test]
fn empty_elements_hidden_ancestors_and_inactive_rules_do_not_download_subsets() {
    assert!(
        requested(
            "<body><p></p><div style='display:none'><span>AB</span></div></body>",
            SUBSETS
        )
        .is_empty()
    );
    let css = format!(
        "{SUBSETS}@media print{{@font-face{{font-family:Print;src:url(print.woff)}}}}p{{font-family:Print}}"
    );
    assert!(requested("<body><p>Text</p></body>", &css).is_empty());
}

#[test]
fn live_control_values_and_generated_content_are_font_usage() {
    let css = format!("{SUBSETS}p::before{{content:'B'}}");
    let fonts = requested(
        "<body><input value=a><textarea>2</textarea><p></p></body>",
        &css,
    );
    assert_eq!(fonts.len(), 3);
    let urls = fonts.iter().map(|font| font.0.as_str()).collect::<Vec<_>>();
    assert!(urls.contains(&"https://example.test/css/upper.woff"));
    assert!(urls.contains(&"https://example.test/css/lower.woff"));
    assert!(urls.contains(&"https://example.test/css/digits.woff"));
}

#[test]
fn matching_uses_transformed_codepoints_for_upper_and_lower_case() {
    let css = format!("{SUBSETS}p{{text-transform:uppercase}}");
    let fonts = requested("<body><p>abc</p></body>", &css);
    assert_eq!(fonts.len(), 1);
    assert!(fonts[0].0.ends_with("upper.woff"));
    let css = format!("{SUBSETS}p{{text-transform:lowercase}}");
    let fonts = requested("<body><p>ABC</p></body>", &css);
    assert_eq!(fonts.len(), 1);
    assert!(fonts[0].0.ends_with("lower.woff"));
}

#[test]
fn a_missing_character_does_not_select_an_inferior_style_subset() {
    let css = r#"
      @font-face{font-family:Weight;src:url(normal.woff);font-weight:400;unicode-range:U+41}
      @font-face{font-family:Weight;src:url(bold.woff);font-weight:700;unicode-range:U+42}
      p{font:700 20px Weight}
    "#;
    assert!(requested("<body><p>A</p></body>", css).is_empty());
    let fonts = requested("<body><p>B</p></body>", css);
    assert_eq!(fonts.len(), 1);
    assert!(fonts[0].0.ends_with("bold.woff"));
    assert_eq!(fonts[0].2, 700);
}

#[test]
fn equal_source_urls_can_install_distinct_subsets_without_collapsing_metadata() {
    let mut page = Page::parse("<body></body>", "https://example.test/");
    let make = |range: &str| WebFontFace {
        family: "Split".into(),
        weight: 400,
        weight_min: 400.0,
        weight_max: 400.0,
        features: Default::default(),
        italic: false,
        url: "https://example.test/ahem.ttf".into(),
        fallback_urls: Vec::new(),
        unicode_range: range.into(),
    };
    let bytes = include_bytes!("../../../../tests/canvas/fonts/ahem.ttf");
    page.add_font_face(make("U+41-5A"), bytes).unwrap();
    page.add_font_face(make("U+61-7A"), bytes).unwrap();
    page.add_font_face(make("U+41-5A"), bytes).unwrap();
    assert_eq!(page.fonts.len(), 2);
    assert!(page.fonts[0].unicode_ranges.contains(0x41));
    assert!(!page.fonts[0].unicode_ranges.contains(0x61));
    assert!(page.fonts[1].unicode_ranges.contains(0x61));
}
