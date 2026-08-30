use super::super::test_support::FixedMeasurer;
use super::super::*;

#[test]
fn video_is_a_replaced_element_and_installs_bounded_frames() {
    let mut page = Page::parse(
        r#"<style>body{margin:0} video{display:block}</style><video width="320" height="180"></video>"#,
        "https://example.com/",
    );
    let video = page.dom.elements_named("video").next().unwrap();
    let mut measurer = FixedMeasurer;
    let placeholder = layout_page(&page, 800.0, 600.0, &mut measurer);
    assert!(placeholder.items.iter().any(|item| {
        matches!(item, DisplayItem::Image { rect, url, .. }
            if url == crate::engine::page::MEDIA_VIDEO_PLACEHOLDER
                && rect.width == 320.0 && rect.height == 180.0)
    }));

    let key = page
        .install_media_frame(
            video.id(),
            crate::engine::page::DecodedImage {
                width: 2,
                height: 2,
                bgra: vec![0; 16],
            },
        )
        .unwrap();
    let frame = layout_page(&page, 800.0, 600.0, &mut measurer);
    assert!(frame.items.iter().any(|item| {
        matches!(item, DisplayItem::Image { rect, url, .. }
            if url == &key && rect.width == 320.0 && rect.height == 180.0)
    }));
}
