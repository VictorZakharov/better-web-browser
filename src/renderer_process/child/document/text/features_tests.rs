use super::*;
use crate::engine::css::FontFeatures;

fn fixture_font() -> WebFont {
    let face = crate::engine::font::WebFontFace {
        family: "FeatureFixture".into(),
        weight: 400,
        weight_min: 400.0,
        weight_max: 400.0,
        features: Default::default(),
        italic: false,
        url: "test:features".into(),
        fallback_urls: Vec::new(),
        unicode_range: "U+0-10FFFF".into(),
    };
    crate::engine::font::decode_web_font(&face, &crate::engine::font::test_features::font_bytes())
        .unwrap()
}

fn fixture() -> (RendererTextSystem, FontSpec) {
    let font = fixture_font();
    let mut text = RendererTextSystem::new(96);
    text.register_web_fonts(&[font]);
    let spec = FontSpec {
        family: "FeatureFixture".into(),
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
    };
    (text, spec)
}

#[test]
fn font_feature_settings_change_actual_ligature_glyphs_and_utf16_clusters() {
    let (mut text, mut spec) = fixture();
    let default = text.shape("fi", &spec);
    assert_eq!(default.glyphs.len(), 1, "real GSUB ligature");
    assert_eq!(default.geometry.clusters.len(), 1);
    assert_eq!(
        (
            default.geometry.clusters[0].start,
            default.geometry.clusters[0].end
        ),
        (0, 2)
    );
    spec.features = FontFeatures::parse("'liga' off").unwrap();
    let disabled = text.shape("fi", &spec);
    assert_eq!(disabled.glyphs.len(), 2, "actual separate f and i glyphs");
    assert_eq!(disabled.geometry.clusters.len(), 2);
    assert!(disabled.width > default.width);
    assert_eq!(text.measure("fi", &spec).0, disabled.width);
    spec.features = FontFeatures::parse("'liga' on").unwrap();
    let enabled = text.shape("fi", &spec);
    assert_eq!(enabled.width, default.width);
    assert_eq!(enabled.glyphs.len(), 1);
    assert_eq!(
        text.shape("fi", &spec),
        enabled,
        "cache respects full settings"
    );
}

#[test]
fn kerning_property_changes_real_pair_advances_and_low_level_settings_override_it() {
    let (mut text, mut spec) = fixture();
    let kerned = text.measure("AV", &spec).0;
    spec.kerning = false;
    let unkerned = text.measure("AV", &spec).0;
    assert!(kerned < unkerned, "fixture kern table must affect advances");
    spec.features = FontFeatures::parse("'kern' on").unwrap();
    assert_eq!(text.measure("AV", &spec).0, kerned);
    spec.kerning = true;
    spec.features = FontFeatures::parse("'kern' off").unwrap();
    assert_eq!(text.measure("AV", &spec).0, unkerned);
}

#[test]
fn spacing_disables_optional_ligatures_but_explicit_low_level_feature_wins() {
    let (mut text, mut spec) = fixture();
    spec.letter_spacing = 1.0;
    assert_eq!(text.shape("fi", &spec).glyphs.len(), 2);
    spec.features = FontFeatures::parse("'liga' on").unwrap();
    assert_eq!(text.shape("fi", &spec).glyphs.len(), 1);
}

#[test]
fn resolved_kerning_policy_overrides_face_defaults_in_both_directions() {
    let (mut text, mut spec) = fixture();
    let kerned = text.measure("AV", &spec).0;
    spec.kerning = false;
    let unkerned = text.measure("AV", &spec).0;
    for descriptor in ["'kern' off", "'kern' on"] {
        let mut font = fixture_font();
        font.features = FontFeatures::parse(descriptor).unwrap();
        text.register_web_fonts(&[font]);
        for enabled in [true, false] {
            spec.kerning = enabled;
            spec.features = FontFeatures::default();
            assert_eq!(
                text.measure("AV", &spec).0,
                if enabled { kerned } else { unkerned },
                "resolved property must override {descriptor}"
            );
            spec.features =
                FontFeatures::parse(if enabled { "'kern' off" } else { "'kern' on" }).unwrap();
            assert_eq!(
                text.measure("AV", &spec).0,
                if enabled { unkerned } else { kerned },
                "explicit low-level settings remain last"
            );
        }
    }
}

#[test]
fn font_face_descriptors_precede_properties_and_metadata_mutation_clears_shaped_runs() {
    let (mut text, mut spec) = fixture();
    let mut font = fixture_font();
    font.features = FontFeatures::parse("'liga' off").unwrap();
    text.register_web_fonts(&[font.clone()]);
    assert_eq!(text.shape("fi", &spec).glyphs.len(), 2);
    spec.features = FontFeatures::parse("'liga' on").unwrap();
    assert_eq!(
        text.shape("fi", &spec).glyphs.len(),
        1,
        "CSS low-level property wins"
    );
    spec.features = FontFeatures::default();
    font.features = FontFeatures::parse("'liga' on").unwrap();
    text.register_web_fonts(&[font]);
    assert_eq!(
        text.shape("fi", &spec).glyphs.len(),
        1,
        "same bytes, changed descriptor"
    );
}

#[test]
fn high_level_common_ligatures_change_real_glyphs_but_explicit_features_win() {
    let (mut text, mut spec) = fixture();
    spec.variants = crate::engine::css::FontVariants::new(
        crate::engine::css::FontLigatures::parse("no-common-ligatures").unwrap(),
        Default::default(),
    );
    assert_eq!(text.shape("fi", &spec).glyphs.len(), 2);
    spec.features = FontFeatures::parse("'liga' on").unwrap();
    assert_eq!(text.shape("fi", &spec).glyphs.len(), 1);
    spec.features = FontFeatures::default();
    spec.variants = Default::default();
    assert_eq!(text.shape("fi", &spec).glyphs.len(), 1);
}

#[test]
fn numeric_variants_select_actual_glyphs_and_explicit_settings_override_them() {
    let (mut text, mut spec) = fixture();
    let lining = text.shape("1", &spec);
    let replacement = text.shape("C", &spec);
    assert_ne!(lining.glyphs[0].raster_id, replacement.glyphs[0].raster_id);
    spec.variants = crate::engine::css::FontVariants::new(
        Default::default(),
        crate::engine::css::FontNumeric::parse("oldstyle-nums").unwrap(),
    );
    let oldstyle = text.shape("1", &spec);
    assert_eq!(
        oldstyle.glyphs[0].raster_id,
        replacement.glyphs[0].raster_id
    );
    spec.features = FontFeatures::parse("'onum' off").unwrap();
    assert_eq!(
        text.shape("1", &spec).glyphs[0].raster_id,
        lining.glyphs[0].raster_id
    );
    spec.features = FontFeatures::default();
    spec.variants = crate::engine::css::FontVariants::new(
        Default::default(),
        crate::engine::css::FontNumeric::parse("lining-nums").unwrap(),
    );
    assert_eq!(
        text.shape("1", &spec).glyphs[0].raster_id,
        lining.glyphs[0].raster_id
    );
}
