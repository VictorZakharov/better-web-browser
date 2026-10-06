use super::*;

fn page(sources: &str) -> Page {
    let source = format!(
        r#"<style>@font-face{{font-family:Remote;src:{sources};
        unicode-range:U+41-5A;font-feature-settings:'liga' off}}
        p{{font-family:Remote;font-size:20px}}</style><body><p>A</p>"#
    );
    let mut page = Page::parse(&source, "https://example.test/base/");
    page.refresh_resources_for_viewport(800.0, 600.0);
    page
}

fn first(page: &Page) -> PageResource {
    page.resources
        .iter()
        .find(|resource| matches!(resource, PageResource::Font { .. }))
        .unwrap()
        .clone()
}

fn font_url(resource: &PageResource) -> &str {
    match resource {
        PageResource::Font { url, .. } => url,
        _ => panic!("font expected"),
    }
}

#[test]
fn alternatives_are_queued_one_at_a_time_with_stable_identity_and_matching_metadata() {
    let mut page = page("url(first.woff),url(second.woff2),url(last.ttf)");
    let original = first(&page);
    assert_eq!(font_url(&original), "https://example.test/base/first.woff");
    assert!(page.retry_font_resource(&original));
    let second = page.resources.last().unwrap().clone();
    assert_eq!(font_url(&second), "https://example.test/base/second.woff2");
    if let PageResource::Font {
        source_url,
        fallback_urls,
        family,
        unicode_range,
        font_feature_settings,
        ..
    } = &second
    {
        assert_eq!(source_url, "https://example.test/base/first.woff");
        assert_eq!(
            fallback_urls.as_slice(),
            ["https://example.test/base/last.ttf"]
        );
        assert_eq!(family, "Remote");
        assert_eq!(unicode_range, "U+41-5A");
        assert_eq!(font_feature_settings, "\"liga\" 0");
    }
    assert!(page.retry_font_resource(&second));
    let last = page.resources.last().unwrap().clone();
    assert_eq!(font_url(&last), "https://example.test/base/last.ttf");
    assert!(!page.retry_font_resource(&last));
    assert_eq!(
        page.resources
            .iter()
            .filter(|r| matches!(r, PageResource::Font { .. }))
            .count(),
        3
    );
}

#[test]
fn repeated_completion_cannot_queue_duplicate_candidates_or_restart_an_exhausted_list() {
    let mut page = page("url(first.ttf),url(second.ttf)");
    let original = first(&page);
    assert!(page.retry_font_resource(&original));
    let size = page.resources.len();
    assert!(page.retry_font_resource(&original));
    assert_eq!(page.resources.len(), size);
    let last = page.resources.last().unwrap().clone();
    assert!(!page.retry_font_resource(&last));
    page.refresh_resources_for_viewport(800.0, 600.0);
    assert_eq!(
        page.resources.len(),
        size,
        "refresh does not restart candidate discovery"
    );
}

#[test]
fn unsupported_hints_and_unavailable_local_aliases_never_become_fetches() {
    let mut page = page("url(skip.ttf) format(unsupported),local('Missing Face'),url(good)");
    let resource = first(&page);
    assert_eq!(font_url(&resource), "https://example.test/base/good");
    assert!(!page.retry_font_resource(&resource));
}

#[test]
fn removed_disabled_or_descriptor_mutated_rules_stop_fallback() {
    for edit in [
        "",
        "@font-face{font-family:Different;src:url(first.ttf),url(second.ttf)}",
        "@font-face{font-family:Remote;src:url(first.ttf),url(second.ttf);unicode-range:U+61-7A}",
        "@font-face{font-family:Remote;src:url(first.ttf),url(second.ttf);font-feature-settings:'liga' on}",
        "@font-face{font-family:Remote;src:url(first.ttf),url(changed.ttf);unicode-range:U+41-5A;font-feature-settings:'liga' off}",
    ] {
        let mut page = page("url(first.ttf),url(second.ttf)");
        let original = first(&page);
        let node = page.dom.elements_named("style").next().unwrap();
        crate::engine::dom::Node::set_text_content(&node, edit);
        assert!(!page.is_current_font_resource(&original), "{edit}");
        assert!(!page.retry_font_resource(&original), "{edit}");
    }
    let mut page = page("url(first.ttf),url(second.ttf)");
    let original = first(&page);
    let node = page.dom.elements_named("style").next().unwrap();
    node.set_attr("media", "not all");
    assert!(!page.retry_font_resource(&original));
}

#[test]
fn later_candidate_decode_keeps_original_face_identity_features_and_coverage() {
    let mut page = page("url(first.ttf),url(second.ttf)");
    let original = first(&page);
    assert!(page.retry_font_resource(&original));
    let PageResource::Font {
        source_url,
        family,
        weight,
        italic,
        unicode_range,
        font_feature_settings,
        ..
    } = page.resources.last().unwrap().clone()
    else {
        unreachable!()
    };
    let face = crate::engine::font::WebFontFace {
        family,
        weight,
        weight_min: f32::from(weight),
        weight_max: f32::from(weight),
        italic,
        url: source_url,
        fallback_urls: Vec::new(),
        unicode_range,
        features: crate::engine::css::FontFeatures::parse(&font_feature_settings).unwrap(),
    };
    page.add_font_face(
        face,
        include_bytes!("../../../../../tests/canvas/fonts/ahem.ttf"),
    )
    .unwrap();
    assert_eq!(page.fonts.len(), 1);
    assert_eq!(page.fonts[0].source_url, font_url(&original));
    assert_eq!(page.fonts[0].features.css_text(), "\"liga\" 0");
    assert!(page.fonts[0].unicode_ranges.contains(u32::from('A')));
    assert!(!page.fonts[0].unicode_ranges.contains(u32::from('a')));
}
