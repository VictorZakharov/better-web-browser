use super::*;

#[test]
fn app_container_shapes_mixed_scripts_into_validated_glyph_resources() {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut session = RendererSession::launch(options()).expect("launch renderer");
    let html = r#"<!doctype html><title>mixed text</title><p>
        office affinity · مرحبا بالعالم · नमस्ते दुनिया · Café Å · ffi fi fl ·
        Hello 👩🏽‍💻 🌍 · English العربية हिन्दी 123
        </p>"#;
    let presentation = load_html_document(&session, 90, html);
    assert_ne!(presentation.glyph_epoch, 0);
    assert!(!presentation.glyphs.is_empty());
    let resources = presentation
        .glyphs
        .iter()
        .map(|glyph| glyph.id)
        .collect::<std::collections::HashSet<_>>();
    let text_items = presentation.layout.items.iter().filter_map(|item| {
        let DisplayItem::Text {
            text,
            raster_run_id,
            glyphs,
            ..
        } = item
        else {
            return None;
        };
        Some((text, *raster_run_id, glyphs))
    });
    let mut visible_items = 0;
    for (text, raster_run_id, glyphs) in text_items {
        if text.chars().any(|character| !character.is_whitespace()) {
            visible_items += 1;
            assert_ne!(raster_run_id, 0, "missing raster-run identity for {text:?}");
            assert!(!glyphs.is_empty(), "missing glyph run for {text:?}");
            assert!(
                glyphs
                    .iter()
                    .all(|glyph| resources.contains(&glyph.raster_id)),
                "glyph run references an unpublished raster"
            );
        }
    }
    assert!(visible_items >= 7);
    session.cancel_document(presentation.document).unwrap();
    let next = load_html_document(&session, 91, html);
    assert_ne!(next.glyph_epoch, presentation.glyph_epoch);
    assert!(
        !next.glyphs.is_empty(),
        "new document did not republish its renderer-owned glyph rasters"
    );
    session.shutdown().expect("shutdown renderer");
}
