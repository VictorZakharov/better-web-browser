use super::*;

fn spec() -> FontSpec {
    FontSpec {
        family: "sans-serif".into(),
        size: 18.0,
        weight: 400,
        italic: false,
        underline: false,
        letter_spacing: 0.0,
        word_spacing: 0.0,
    }
}

#[test]
fn shapes_and_rasterizes_representative_scripts_deterministically() {
    let fixtures = [
        "office affinity",
        "\u{0645}\u{0631}\u{062d}\u{0628}\u{0627} \u{0628}\u{0627}\u{0644}\u{0639}\u{0627}\u{0644}\u{0645}",
        "\u{0928}\u{092e}\u{0938}\u{094d}\u{0924}\u{0947} \u{0926}\u{0941}\u{0928}\u{093f}\u{092f}\u{093e}",
        "Cafe\u{301} A\u{30a}",
        "ffi fi fl",
        "Hello \u{1f469}\u{1f3fd}\u{200d}\u{1f4bb} \u{1f30d}",
        "English \u{0627}\u{0644}\u{0639}\u{0631}\u{0628}\u{064a}\u{0629} \u{0939}\u{093f}\u{0928}\u{094d}\u{0926}\u{0940} 123",
    ];
    let mut text = RendererTextSystem::new(96);
    for fixture in fixtures {
        let first = text.shape(fixture, &spec());
        let second = text.shape(fixture, &spec());
        assert_eq!(first, second, "nondeterministic geometry for {fixture}");
        assert!(first.width > 0.0 && first.height > 0.0);
        assert!(!first.glyphs.is_empty(), "no visible glyphs for {fixture}");
        assert!(first.glyphs.iter().all(|glyph| {
            glyph.x.is_finite() && glyph.y.is_finite() && glyph.width > 0.0 && glyph.height > 0.0
        }));
    }
    assert!(!text.take_pending_glyphs().is_empty());
}

#[test]
fn css_spacing_changes_shaped_geometry() {
    let mut text = RendererTextSystem::new(96);
    let normal = text.shape("a b", &spec());
    let mut spaced = spec();
    spaced.letter_spacing = 1.0;
    spaced.word_spacing = 4.0;
    let spaced = text.shape("a b", &spaced);
    assert!(spaced.width > normal.width + 5.0);
}

#[test]
fn registers_bounded_in_memory_font_bytes_under_the_css_family_alias() {
    let mut text = RendererTextSystem::new(96);
    let face = text.fonts.db().faces().next().expect("system font").id;
    let bytes = text
        .fonts
        .db()
        .with_face_data(face, |data, _| data.to_vec())
        .expect("system font bytes");
    text.register_web_fonts(&[WebFont {
        family: "Breeze Test Alias".into(),
        weight: 600,
        italic: false,
        sfnt: bytes,
    }]);
    assert!(text.fonts.db().faces().any(|face| {
        face.families
            .iter()
            .any(|(family, _)| family == "Breeze Test Alias")
    }));
    let mut aliased = spec();
    aliased.family = "Breeze Test Alias".into();
    aliased.weight = 600;
    assert!(!text.shape("webfont", &aliased).glyphs.is_empty());
}

#[test]
fn navigation_advances_the_epoch_and_reemits_system_font_rasters() {
    let mut text = RendererTextSystem::new(96);
    let first = text.shape("reused text", &spec());
    let first_epoch = text.glyph_epoch();
    assert!(!text.take_pending_glyphs().is_empty());

    text.reset_for_navigation();
    assert_ne!(text.glyph_epoch(), first_epoch);
    let second = text.shape("reused text", &spec());

    assert_eq!(first.width, second.width);
    assert_eq!(first.height, second.height);
    assert!(!text.take_pending_glyphs().is_empty());
}
