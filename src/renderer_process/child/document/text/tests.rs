use super::*;

#[test]
fn retained_advance_clusters_reuse_measurement_without_rasterizing() {
    let mut text = RendererTextSystem::new(96);
    let font = spec();
    let measured = text.measure("a🌠e\u{301} שלום", &font);
    assert!(text.take_pending_glyphs().is_empty());
    let shaped_time = text.open_type_time;
    let geometry = text.text_geometry("a🌠e\u{301} שלום", &font);
    assert_eq!(
        text.open_type_time, shaped_time,
        "CSSOM reuses the measurement cache"
    );
    assert!(
        text.take_pending_glyphs().is_empty(),
        "geometry never requests pixels"
    );
    assert!((geometry.bounds.width - measured.0).abs() < 0.001);
    assert!(geometry.bounds.height > 0.0);
    assert!(geometry.clusters.iter().any(|c| c.start == 1 && c.end == 3));
    assert!(geometry.clusters.iter().any(|c| c.rtl));
    let painted = text.shape("a🌠e\u{301} שלום", &font);
    assert_eq!(
        painted.geometry, geometry,
        "layout and paint consume identical advances"
    );
    assert_eq!(text.text_geometry("a🌠e\u{301} שלום", &font), geometry);
    assert!(
        shape_cache_entry_bytes(&ShapeKey::new("a🌠e\u{301} שלום", &font), &painted)
            >= geometry_bytes(&geometry)
    );
}

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
fn borrowed_cache_keys_match_owned_entries_without_losing_shaping_inputs() {
    let text = String::from("a b");
    let font = spec();
    let key = ShapeKey::new(&text, &font);
    assert!(matches!(key.text, Cow::Borrowed(_)));
    assert!(matches!(key.family, Cow::Borrowed(_)));
    let owned = key.clone().into_owned();
    let cache: HashMap<ShapeKey<'static>, u32> = HashMap::from([(owned, 42)]);
    assert_eq!(cache.get(&key), Some(&42));
    assert_eq!(cache.get(&ShapeKey::new("different", &font)), None);
    for field in 0..6 {
        let mut changed = font.clone();
        match field {
            0 => changed.family = "serif".into(),
            1 => changed.size += 1.0,
            2 => changed.weight += 100,
            3 => changed.italic = !changed.italic,
            4 => changed.letter_spacing += 1.0,
            _ => changed.word_spacing += 1.0,
        }
        assert_eq!(cache.get(&ShapeKey::new(&text, &changed)), None, "{field}");
    }
}

#[test]
fn reports_consume_work_counters_without_discarding_cached_shapes() {
    let mut text = RendererTextSystem::new(96);
    text.shape("cached text", &spec());
    let first = text.finish_load_report(PageLoadReport::default());
    assert!(first.text_measure_count > 0);
    assert!(first.text_shape_cache_entries > 0);
    let idle = text.finish_load_report(PageLoadReport::default());
    assert_eq!(idle.text_measure_count, 0);
    assert_eq!(idle.text_shape_cache_hits, 0);
    assert_eq!(idle.text_shape_cache_misses, 0);
    assert_eq!(idle.font_select_micros, 0);
    assert_eq!(idle.open_type_shape_micros, 0);
    assert_eq!(idle.glyph_raster_micros, 0);
    assert_eq!(
        idle.text_shape_cache_entries,
        first.text_shape_cache_entries
    );
    text.shape("cached text", &spec());
    let reused = text.finish_load_report(PageLoadReport::default());
    assert_eq!(reused.text_shape_cache_hits, 1);
    assert_eq!(reused.text_shape_cache_misses, 0);
}

#[test]
fn unavailable_first_family_falls_back_to_the_next_named_family() {
    let mut text = RendererTextSystem::new(96);
    let mut direct = spec();
    direct.family = "Georgia, serif".into();
    let expected = text.shape("A serif heading", &direct);
    let mut fallback = direct;
    fallback.family = "'Absent Fixture Font', Georgia, serif".into();
    let actual = text.shape("A serif heading", &fallback);
    assert!(!actual.glyphs.is_empty());
    assert_eq!(actual.width, expected.width);
    assert_eq!(
        actual
            .glyphs
            .iter()
            .map(|g| g.raster_id)
            .collect::<Vec<_>>(),
        expected
            .glyphs
            .iter()
            .map(|g| g.raster_id)
            .collect::<Vec<_>>()
    );
}

#[test]
fn measurement_shapes_advances_without_rasterizing_glyphs() {
    let mut text = RendererTextSystem::new(96);
    let font = spec();

    let measured = text.measure("geometry only", &font);

    assert!(measured.0 > 0.0);
    assert!(measured.1 > 0.0);
    assert!(text.take_pending_glyphs().is_empty());

    let shaped = text.shape("geometry only", &font);
    assert_eq!((shaped.width, shaped.height), measured);
    assert!(!shaped.glyphs.is_empty());
    assert!(!text.take_pending_glyphs().is_empty());
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
fn preserves_contextual_arabic_and_canonical_mark_geometry() {
    let mut text = RendererTextSystem::new(96);
    let joined = text.shape("\u{0633}\u{0644}\u{0627}\u{0645}", &spec());
    let join_blocked = text.shape(
        "\u{0633}\u{200c}\u{0644}\u{200c}\u{0627}\u{200c}\u{0645}",
        &spec(),
    );
    let joined_rasters = joined
        .glyphs
        .iter()
        .map(|glyph| glyph.raster_id)
        .collect::<Vec<_>>();
    let blocked_rasters = join_blocked
        .glyphs
        .iter()
        .map(|glyph| glyph.raster_id)
        .collect::<Vec<_>>();
    assert_ne!(
        joined_rasters, blocked_rasters,
        "Arabic joining context did not affect selected glyph forms"
    );

    let composed = text.shape("Caf\u{e9}", &spec());
    let decomposed = text.shape("Cafe\u{301}", &spec());
    assert!(
        (composed.width - decomposed.width).abs() < 0.25,
        "canonical combining-mark geometry diverged"
    );
}

#[test]
fn registers_bounded_in_memory_font_bytes_under_the_css_family_alias() {
    let mut text = RendererTextSystem::new(96);
    let bytes = text
        .catalog
        .first_system_font_bytes()
        .expect("system font bytes");
    text.register_web_fonts(&[WebFont {
        family: "Breeze Test Alias".into(),
        weight: 600,
        italic: false,
        sfnt: bytes,
        source_url: "test-font:alias".into(),
        script_source_id: None,
    }]);
    assert!(text.catalog.contains_family("Breeze Test Alias"));
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
