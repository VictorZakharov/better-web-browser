use super::*;
use crate::engine::dom::Node;

mod invalidation;
mod scripts;

#[test]
fn discovers_and_resolves_page_resources() {
    let page = Page::parse(
        r#"
                <base href="https://cdn.example/assets/">
                <link rel="alternate stylesheet" href="theme.css">
                <img src="logo.png"><img src="logo.png">
            "#,
        "https://example.com/start",
    );
    assert_eq!(
        page.resources,
        vec![
            PageResource::Stylesheet {
                url: "https://cdn.example/assets/theme.css".into()
            },
            PageResource::Image {
                url: "https://cdn.example/assets/logo.png".into()
            }
        ]
    );
}

#[test]
fn admits_one_supported_video_source_for_the_document_playback_clock() {
    let page = Page::parse(
        r#"<video><source src="movie.webm" type="video/webm"><source src="movie.mp4" type="video/mp4"></video>
            <video src="replacement.mp4"></video>"#,
        "https://example.com/watch/",
    );
    let media = page
        .resources
        .iter()
        .filter(|resource| matches!(resource, PageResource::Media { .. }))
        .collect::<Vec<_>>();
    assert_eq!(media.len(), 1);
    assert!(matches!(
        media[0],
        PageResource::Media { url, .. } if url == "https://example.com/watch/movie.mp4"
    ));
}

#[test]
fn discovers_stylesheets_and_images_inside_shadow_trees() {
    let mut page = Page::parse("<x-card></x-card>", "https://example.com/app/");
    let host = page.dom.elements_named("x-card").next().unwrap();
    let root = Node::attach_shadow(
        &host,
        crate::engine::dom::ShadowRootMode::Open,
        false,
        false,
        false,
    )
    .unwrap();
    let stylesheet = Node::create_element_for(&root, "link");
    stylesheet.set_attr("rel", "stylesheet");
    stylesheet.set_attr("href", "components/card.css");
    Node::append_child(&root, stylesheet);
    let image = Node::create_element_for(&root, "img");
    image.set_attr("src", "images/story.jpg");
    Node::append_child(&root, image);

    page.refresh_resources(800.0);

    assert!(page.resources.contains(&PageResource::Stylesheet {
        url: "https://example.com/app/components/card.css".into()
    }));
    assert!(page.resources.contains(&PageResource::Image {
        url: "https://example.com/app/images/story.jpg".into()
    }));
}

#[test]
fn prefers_lazy_and_high_density_image_sources_over_placeholders() {
    let page = Page::parse(
        r#"<img src="data:image/svg+xml,placeholder" data-src="portrait.jpg">
               <img src="small.jpg" srcset="small.jpg 1x, large.jpg 2x">"#,
        "https://example.com/posts/",
    );
    assert!(page.resources.contains(&PageResource::Image {
        url: "https://example.com/posts/portrait.jpg".into()
    }));
    assert!(page.resources.contains(&PageResource::Image {
        url: "https://example.com/posts/large.jpg".into()
    }));
}

#[test]
fn selects_picture_sources_by_media_type_and_viewport() {
    let mut page = Page::parse(
        r#"<picture>
                <source type="image/avif" srcset="unsupported.avif">
                <source media="(max-width: 600px)" srcset="phone.jpg">
                <source media="(min-width: 601px)" srcset="desktop.webp" type="image/webp">
                <img src="fallback.jpg" alt="responsive">
            </picture>"#,
        "https://example.com/images/",
    );
    let image = page.dom.elements_named("img").next().unwrap();
    assert_eq!(
        page.image_url(&image).as_deref(),
        Some("https://example.com/images/desktop.webp")
    );

    page.refresh_resources(500.0);
    assert_eq!(
        page.image_url(&image).as_deref(),
        Some("https://example.com/images/phone.jpg")
    );
    assert!(page.resources.contains(&PageResource::Image {
        url: "https://example.com/images/phone.jpg".into()
    }));
}

#[test]
fn uses_sizes_to_choose_width_described_srcset_candidates() {
    let mut page = Page::parse(
        r#"<img sizes="(max-width: 600px) 100vw, 50vw"
                     srcset="small.jpg 400w, medium.jpg 800w, large.jpg 1600w"
                     src="fallback.jpg">"#,
        "https://example.com/",
    );
    page.refresh_resources(400.0);
    let image = page.dom.elements_named("img").next().unwrap();
    assert_eq!(
        page.image_url(&image).as_deref(),
        Some("https://example.com/medium.jpg")
    );
}

#[test]
fn requests_only_webfont_faces_used_by_computed_styles() {
    let mut page = Page::parse("<body><strong>text</strong></body>", "https://example.com/");
    page.add_stylesheet_from(
        "https://example.com/css/main.css",
        r#"
                @font-face { font-family: Used; font-weight: 400;
                    src: url(../fonts/used.woff) format("woff"); }
                @font-face { font-family: Used; font-weight: 700;
                    src: url(../fonts/used-bold.woff) format("woff"); }
                @font-face { font-family: Unused;
                    src: url(../fonts/unused.woff) format("woff"); }
                body { font-family: Used; }
            "#
        .into(),
    );
    page.refresh_resources(800.0);
    let fonts = page
        .resources
        .iter()
        .filter_map(|resource| match resource {
            PageResource::Font { url, .. } => Some(url.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        fonts,
        vec![
            "https://example.com/fonts/used.woff",
            "https://example.com/fonts/used-bold.woff"
        ]
    );
}

#[test]
fn discovers_background_images_from_computed_styles() {
    let mut page = Page::parse(
        r#"<a class="logo"></a><span class="icon"></span>"#,
        "https://example.com/articles/page.html",
    );
    page.add_stylesheet_from(
        "https://cdn.example/css/site.css",
        ".logo { background: no-repeat center url(../images/logo.svg) }
         .icon { mask-image: url(../images/menu.svg) }"
            .into(),
    );
    page.refresh_resources(800.0);
    assert!(page.resources.contains(&PageResource::Image {
        url: "https://cdn.example/images/logo.svg".into()
    }));
    assert!(page.resources.contains(&PageResource::Image {
        url: "https://cdn.example/images/menu.svg".into()
    }));
}

#[test]
fn decodes_images_to_bgra() {
    let mut page = Page::parse("", "https://example.com/");
    let source = image::RgbaImage::from_pixel(1, 1, image::Rgba([12, 34, 56, 255]));
    let mut png = Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(source)
        .write_to(&mut png, image::ImageFormat::Png)
        .unwrap();
    page.add_image("https://example.com/a.png".into(), png.get_ref())
        .unwrap();
    let image = &page.images["https://example.com/a.png"];
    assert_eq!((image.width, image.height), (1, 1));
    assert_eq!(image.bgra, vec![56, 34, 12, 255]);
}

#[test]
fn rasterizes_external_svg_images() {
    let mut page = Page::parse("", "https://example.com/");
    page.add_image(
            "https://example.com/logo.svg".into(),
            br#"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="10"><rect width="20" height="10" fill="red"/></svg>"#,
        )
        .unwrap();
    let image = &page.images["https://example.com/logo.svg"];
    assert_eq!((image.width, image.height), (20, 10));
    assert!(image.bgra.chunks_exact(4).any(|pixel| pixel[3] != 0));
}

#[test]
fn decodes_embedded_css_mask_images_without_networking() {
    let mut page = Page::parse(r#"<span class="icon"></span>"#, "https://example.com/");
    page.add_stylesheet(
        ".icon { mask-image: url('data:image/svg+xml,%3Csvg xmlns=%22http://www.w3.org/2000/svg%22 width=%2210%22 height=%2210%22%3E%3Cpath d=%22M0 0h5v10H0z%22/%3E%3C/svg%3E') }".into(),
    );
    page.refresh_resources(800.0);

    let mask = page
        .style(800.0)
        .get(&page.dom.elements_named("span").next().unwrap())
        .mask_image
        .clone()
        .unwrap();
    let image = &page.images[&mask];
    assert_eq!((image.width, image.height), (10, 10));
    assert!(image.bgra.chunks_exact(4).any(|pixel| pixel[3] == 0));
    assert!(image.bgra.chunks_exact(4).any(|pixel| pixel[3] != 0));
}

#[test]
fn rasterizes_inline_svg_without_a_browser_runtime() {
    let page = Page::parse(
        r#"<svg viewBox="0 0 24 24"><path d="M4 4h16v16H4z"/></svg>"#,
        "https://example.com/",
    );
    let svg = page.dom.elements_named("svg").next().unwrap();
    let image = &page.images[&inline_svg_key(&svg)];
    assert_eq!((image.width, image.height), (24, 24));
    assert!(image.bgra.chunks_exact(4).any(|pixel| pixel[3] != 0));
}

#[test]
fn resolves_immediate_meta_refresh_against_the_document_base() {
    let page = Page::parse(
        r#"
                <base href="https://example.com/base/">
                <meta http-equiv="refresh" content="0; URL='../landing?q=1&amp;x=2'">
            "#,
        "https://example.com/start",
    );
    assert_eq!(
        page.immediate_refresh_url().as_deref(),
        Some("https://example.com/landing?q=1&x=2")
    );
}

#[test]
fn ignores_delayed_meta_refresh_for_immediate_navigation() {
    let page = Page::parse(
        r#"<meta http-equiv="refresh" content="5;url=/later">"#,
        "https://example.com/start",
    );
    assert_eq!(page.immediate_refresh_url(), None);
}
