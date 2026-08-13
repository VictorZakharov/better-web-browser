use super::*;

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
        r#"<a class="logo"></a>"#,
        "https://example.com/articles/page.html",
    );
    page.add_stylesheet_from(
        "https://cdn.example/css/site.css",
        ".logo { background: no-repeat center url(../images/logo.svg) }".into(),
    );
    page.refresh_resources(800.0);
    assert!(page.resources.contains(&PageResource::Image {
        url: "https://cdn.example/images/logo.svg".into()
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

#[test]
fn discovers_external_scripts_and_executes_dom_mutations() {
    let mut page = Page::parse_scripted(
        r#"
                <body><main id="app"></main>
                <script src="/library.js"></script>
                <script>
                    const item = document.createElement('p');
                    item.textContent = libraryMessage;
                    document.getElementById('app').appendChild(item);
                </script>
            "#,
        "https://example.com/start",
    );
    assert!(page.resources.contains(&PageResource::Script {
        url: "https://example.com/library.js".into()
    }));
    page.add_script(
        "https://example.com/library.js",
        "const libraryMessage = 'loaded';".into(),
    );
    let outcome = page.execute_scripts();
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(outcome.executed, 2);
    assert_eq!(
        page.dom.elements_named("p").next().unwrap().text_content(),
        "loaded"
    );
}

#[test]
fn keeps_async_scripts_off_the_first_paint_path() {
    let mut page = Page::parse_scripted(
        r#"
                <body><div id="status">initial</div>
                <script async src="/analytics.js"></script>
                <script src="/application.js"></script>
            "#,
        "https://example.com/",
    );
    let analytics = PageResource::Script {
        url: "https://example.com/analytics.js".into(),
    };
    let application = PageResource::Script {
        url: "https://example.com/application.js".into(),
    };
    assert!(!page.resource_blocks_first_paint(&analytics));
    assert!(page.resource_blocks_first_paint(&application));

    page.add_script(
        "https://example.com/analytics.js",
        "document.getElementById('status').textContent = 'analytics';".into(),
    );
    page.add_script(
        "https://example.com/application.js",
        "document.getElementById('status').textContent = 'application';".into(),
    );
    let outcome = page.execute_first_paint_scripts();
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(outcome.executed, 1);
    assert_eq!(
        page.dom
            .elements_named("div")
            .next()
            .unwrap()
            .text_content(),
        "application"
    );
}

#[test]
fn retained_first_paint_runtime_mutates_the_same_page_after_load() {
    let mut page = Page::parse_scripted(
        r#"
                <body><div id="status">initial</div>
                <script>
                    globalThis.runtimeMarker = 41;
                    setTimeout(() => {
                        runtimeMarker += 1;
                        document.getElementById('status').textContent = `updated ${runtimeMarker}`;
                    }, 2000);
                </script>
            "#,
        "https://example.com/",
    );
    let mut unused_loader = |_url: &str| Err("unexpected dynamic script".to_string());
    let (runtime, initial) = page.start_first_paint_script_runtime_with_loader(&mut unused_loader);
    let mut runtime = runtime.expect("a loaded script should retain its realm");

    assert!(initial.errors.is_empty(), "{:?}", initial.errors);
    assert_eq!(
        page.dom
            .elements_named("div")
            .next()
            .unwrap()
            .text_content(),
        "initial"
    );
    assert_eq!(
        runtime.next_timer_delay(),
        Some(std::time::Duration::from_millis(500))
    );

    let post_load = runtime.advance_time(std::time::Duration::from_millis(500), 128);
    assert!(post_load.errors.is_empty(), "{:?}", post_load.errors);
    assert!(post_load.render_requested);
    assert_eq!(post_load.mutation_count, 1);
    assert_eq!(
        page.dom
            .elements_named("div")
            .next()
            .unwrap()
            .text_content(),
        "updated 42"
    );
}
