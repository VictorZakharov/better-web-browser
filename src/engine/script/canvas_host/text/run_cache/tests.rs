use super::*;

fn font() -> FontSpec {
    FontSpec {
        family: "Arial".into(),
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
fn key(text: &str) -> RunKey {
    RunKey::new(text, &font(), None, &ShapeOptions::default())
}
fn value(bytes: usize) -> JsValue {
    JsValue::Array(vec![JsValue::from(12), JsValue::Bytes(vec![128; bytes])])
}

#[test]
fn every_shaping_and_raster_input_participates_in_the_key() {
    let original = key("ABC");
    let mut variants = Vec::new();
    variants.push(key("ABD"));
    let mut fonts = Vec::new();
    let mut changed = font();
    changed.family = "serif".into();
    fonts.push(changed);
    let mut changed = font();
    changed.size = 21.0;
    fonts.push(changed);
    let mut changed = font();
    changed.weight = 700;
    fonts.push(changed);
    let mut changed = font();
    changed.italic = true;
    fonts.push(changed);
    let mut changed = font();
    changed.letter_spacing = 1.0;
    fonts.push(changed);
    let mut changed = font();
    changed.word_spacing = 2.0;
    fonts.push(changed);
    variants.extend(
        fonts
            .iter()
            .map(|font| RunKey::new("ABC", font, None, &ShapeOptions::default())),
    );
    variants.push(RunKey::new(
        "ABC",
        &font(),
        Some(1.0),
        &ShapeOptions::default(),
    ));
    for options in [
        ShapeOptions {
            rtl: Some(false),
            ..Default::default()
        },
        ShapeOptions {
            rtl: Some(true),
            ..Default::default()
        },
        ShapeOptions {
            language: Some("sr".parse().unwrap()),
            ..Default::default()
        },
        ShapeOptions {
            kerning: false,
            ..Default::default()
        },
    ] {
        variants.push(RunKey::new("ABC", &font(), None, &options));
    }
    let mut cache = RunCache::default();
    cache.insert(original.clone(), &value(40));
    assert_eq!(cache.get(&original), Some(value(40)));
    for key in variants {
        assert!(cache.get(&key).is_none(), "{key:?}");
    }
}

#[test]
fn exported_bytes_are_deep_owned_and_author_mutation_cannot_change_a_hit() {
    let mut cache = RunCache::default();
    let key = key("ABC");
    cache.insert(key.clone(), &value(40));
    let JsValue::Array(mut exported) = cache.get(&key).unwrap() else {
        panic!("expected array")
    };
    let JsValue::Bytes(bytes) = &mut exported[1] else {
        panic!("expected bytes")
    };
    bytes.fill(0);
    exported[0] = JsValue::from(999);
    assert_eq!(cache.get(&key), Some(value(40)));
}

#[test]
fn entry_limit_evicts_least_recently_used_run() {
    let mut cache = RunCache::default();
    for index in 0..MAX_ENTRIES {
        cache.insert(key(&index.to_string()), &value(1));
    }
    assert!(cache.get(&key("0")).is_some());
    cache.insert(key("new"), &value(1));
    assert!(cache.get(&key("1")).is_none());
    assert!(cache.get(&key("0")).is_some());
    assert_eq!(cache.entries.len(), MAX_ENTRIES);
}

#[test]
fn byte_budget_includes_nested_metadata_keys_and_replacement() {
    let mut cache = RunCache::default();
    for index in 0..64 {
        cache.insert(key(&index.to_string()), &value(400_000));
    }
    assert!(cache.bytes <= MAX_BYTES);
    assert!(cache.entries.len() < MAX_ENTRIES);
    let key = key("latest");
    cache.insert(key.clone(), &value(40));
    let before = cache.bytes;
    cache.insert(key.clone(), &value(80));
    assert_eq!(cache.bytes, before + 40);
    assert_eq!(cache.get(&key), Some(value(80)));
    cache.clear();
    assert_eq!(cache.bytes, 0);
    assert!(cache.entries.is_empty());
}

#[test]
fn oversized_runs_and_unexpected_provider_shapes_are_not_retained() {
    let mut cache = RunCache::default();
    cache.insert(key("large"), &value(MAX_ENTRY_BYTES));
    cache.insert(key("object"), &JsValue::Object(vec![]));
    let mut nested = JsValue::Null;
    for _ in 0..10 {
        nested = JsValue::Array(vec![nested]);
    }
    cache.insert(key("nested"), &nested);
    assert!(cache.entries.is_empty());
    assert_eq!(cache.bytes, 0);
}
