use super::*;
use crate::engine::font::decode_web_font;

fn ahem(family: &str, range: &str) -> WebFont {
    let face = WebFontFace {
        family: family.into(),
        weight: 400,
        weight_min: 400.0,
        weight_max: 400.0,
        features: Default::default(),
        italic: false,
        url: "https://example.test/ahem.ttf".into(),
        fallback_urls: Vec::new(),
        unicode_range: "U+0-10FFFF".into(),
    };
    let mut font = decode_web_font(
        &face,
        include_bytes!("../../../../../../tests/canvas/fonts/ahem.ttf"),
    )
    .unwrap();
    font.unicode_ranges = UnicodeRanges::parse(range).unwrap();
    font
}

fn spec(family: &str) -> FontSpec {
    FontSpec {
        family: family.into(),
        size: 20.0,
        weight: 400,
        italic: false,
        underline: false,
        letter_spacing: 0.0,
        word_spacing: 0.0,
        rtl: false,
        kerning: true,
        variants: Default::default(),
        features: Default::default(),
    }
}

#[test]
fn equal_style_subsets_select_distinct_real_blobs_and_excluded_text_falls_back() {
    let mut catalog = FontCatalog::new();
    let upper = ahem("SubsetAhem", "U+41-5A");
    let lower = ahem("SubsetAhem", "U+61-7A");
    catalog.register_web_fonts(&[upper, lower]);
    let spec = spec("SubsetAhem");
    let a = catalog
        .select(&spec.family, &spec, UnicodeScript::Latin, "A")
        .unwrap();
    let b = catalog
        .select(&spec.family, &spec, UnicodeScript::Latin, "b")
        .unwrap();
    let digit = catalog
        .select(&spec.family, &spec, UnicodeScript::Latin, "0")
        .unwrap();
    assert_ne!(a.instance.blob_id, b.instance.blob_id);
    assert_ne!(digit.instance.blob_id, a.instance.blob_id);
    assert_ne!(digit.instance.blob_id, b.instance.blob_id);
    assert!(a.font.charmap().unwrap().map('A').unwrap() != 0);
    assert!(b.font.charmap().unwrap().map('b').unwrap() != 0);
}

#[test]
fn range_mutation_invalidates_font_selection_without_changing_byte_identity() {
    let mut catalog = FontCatalog::new();
    let mut font = ahem("MutableSubset", "U+41");
    let spec = spec("MutableSubset");
    catalog.register_web_fonts(&[font.clone()]);
    let prior = catalog
        .select(&spec.family, &spec, UnicodeScript::Latin, "A")
        .unwrap();
    font.unicode_ranges = UnicodeRanges::parse("U+42").unwrap();
    assert!(catalog.register_web_fonts(&[font.clone()]));
    let next = catalog
        .select(&spec.family, &spec, UnicodeScript::Latin, "A")
        .unwrap();
    assert_ne!(prior.instance.blob_id, next.instance.blob_id);
    assert!(!catalog.register_web_fonts(&[font]));
}

#[test]
fn ignorable_cluster_controls_do_not_require_descriptor_coverage() {
    let mut catalog = FontCatalog::new();
    catalog.register_web_fonts(&[ahem("ControlSubset", "U+41")]);
    let spec = spec("ControlSubset");
    let a = catalog
        .select(&spec.family, &spec, UnicodeScript::Latin, "A")
        .unwrap();
    let joined = catalog
        .select(&spec.family, &spec, UnicodeScript::Latin, "A\u{200d}")
        .unwrap();
    assert_eq!(a.instance, joined.instance);
}
