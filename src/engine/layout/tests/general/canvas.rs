use super::*;

#[test]
fn scripted_canvas_has_intrinsic_replaced_size_and_paints_published_pixels() {
    let mut page = Page::parse_scripted(
        "<style>body{margin:0}</style><canvas width=2 height=1>fallback</canvas>",
        "https://example.test/",
    );
    let canvas = page.dom.elements_named("canvas").next().unwrap();
    let mut measurer = FixedMeasurer;
    let blank = layout_page(&page, 800.0, 600.0, &mut measurer);
    let bounds = blank.node_bounds[&canvas.id()];
    assert_eq!((bounds.width, bounds.height), (2.0, 1.0));
    assert!(
        !blank
            .items
            .iter()
            .any(|item| matches!(item, DisplayItem::Image { .. }))
    );
    assert!(
        !blank.items.iter().any(
            |item| matches!(item, DisplayItem::Text { text, .. } if text.contains("fallback"))
        )
    );

    page.install_canvas_bitmap(canvas.id(), 2, 1, Some(vec![255, 0, 0, 255, 0, 0, 0, 0]))
        .unwrap();
    let painted = layout_page(&page, 800.0, 600.0, &mut measurer);
    assert!(painted.items.iter().any(|item| matches!(item, DisplayItem::Image { url, .. } if url.starts_with("breeze-internal:canvas:"))));
}

#[test]
fn unscripted_canvas_shows_fallback_content() {
    let page = Page::parse("<canvas>fallback</canvas>", "https://example.test/");
    let mut measurer = FixedMeasurer;
    let layout = layout_page(&page, 800.0, 600.0, &mut measurer);
    assert!(
        layout.items.iter().any(
            |item| matches!(item, DisplayItem::Text { text, .. } if text.contains("fallback"))
        )
    );
}
