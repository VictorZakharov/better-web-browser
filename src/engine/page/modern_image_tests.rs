use super::*;
use crate::engine::css::media::MediaEnvironment;
use crate::engine::image_decode::fixtures;

#[test]
fn picture_sources_select_real_modern_formats_and_skip_unsupported_types() {
    for (format, file) in [("image/avif", "modern.avif"), ("image/jxl", "modern.jxl")] {
        let html = format!(
            r#"<picture><source type="image/heic" srcset="unsupported.heic">
            <source type="{format}" srcset="{file}"><img src="fallback.png"></picture>"#
        );
        let page = Page::parse(&html, "https://example.com/");
        let img = page.dom.elements_named("img").next().unwrap();
        assert_eq!(
            page.image_url(&img),
            Some(format!("https://example.com/{file}"))
        );
        assert!(page.resources.contains(&PageResource::Image {
            url: format!("https://example.com/{file}")
        }));
    }
}

#[test]
fn modern_picture_media_and_density_selection_still_use_the_shared_html_algorithm() {
    let mut page = Page::parse(
        r#"<picture>
        <source type="image/avif" media="(min-width: 600px)" srcset="one.avif 1x, two.avif 2x">
        <source type="image/jxl" srcset="small.jxl"><img src="fallback.png"></picture>"#,
        "https://example.com/",
    );
    page.set_media_environment(MediaEnvironment::new(1000.0, 700.0, 2.0, false));
    page.refresh_resources(1000.0);
    let img = page.dom.elements_named("img").next().unwrap();
    assert_eq!(
        page.image_url(&img).as_deref(),
        Some("https://example.com/two.avif")
    );
    page.refresh_resources(500.0);
    assert_eq!(
        page.image_url(&img).as_deref(),
        Some("https://example.com/small.jxl")
    );
}

#[test]
fn image_preloads_share_format_admission_with_picture_selection() {
    for format in ["image/avif", "IMAGE/JXL", "image/png", "image/jpeg"] {
        let html = format!(r#"<link rel="preload" as="image" type="{format}" href="pixels.bin">"#);
        let page = Page::parse(&html, "https://example.com/");
        assert!(
            page.resources.iter().any(
                |resource| matches!(resource,PageResource::Preload{url,as_type:PreloadAs::Image,..}
            if url=="https://example.com/pixels.bin")
            ),
            "{format}"
        );
    }
    let page = Page::parse(
        r#"<link rel="preload" as="image" type="image/heic" href="unsupported.heic">"#,
        "https://example.com/",
    );
    assert!(
        !page
            .resources
            .iter()
            .any(|resource| matches!(resource, PageResource::Preload { .. }))
    );
}

#[test]
fn page_decodes_real_modern_images_and_retains_correct_intrinsic_dimensions() {
    for name in ["rgb-lossless.avif", "rgba-lossless.jxl"] {
        let mut page = Page::parse("<img src='pixels.bin'>", "https://example.com/");
        page.add_image(
            "https://example.com/pixels.bin".into(),
            &fixtures::find(name),
        )
        .unwrap();
        let image = page.images.get("https://example.com/pixels.bin").unwrap();
        assert_eq!((image.width, image.height), (3, 2));
        assert_eq!(image.bgra.len(), 24);
    }
}

#[test]
fn invalid_modern_image_is_diagnostic_not_an_empty_successful_image() {
    let mut page = Page::parse("<img src='pixels.avif'>", "https://example.com/");
    let error = page
        .add_image(
            "https://example.com/pixels.avif".into(),
            b"\0\0\0\x10ftypavif\0\0\0\0",
        )
        .unwrap_err();
    assert!(!page.images.contains_key("https://example.com/pixels.avif"));
    assert!(error.contains("AVIF"));
}
