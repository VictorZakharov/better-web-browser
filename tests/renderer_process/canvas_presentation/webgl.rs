//! Real ANGLE rendering must work inside the AppContainer and survive presentation IPC.
use super::super::support::*;
use super::modern_images::completed;
use better_web_browser::renderer_process::RendererSession;

#[test]
fn webgl_textured_pixels_and_api_contracts_cross_the_contained_renderer() {
    let _serial = SERIAL.lock().unwrap_or_else(|error| error.into_inner());
    let session = RendererSession::launch(options()).expect("launch hidden renderer");
    // Keep verification in the drawing task so its first presentation owns the
    // two new bitmaps, rather than checking a later text-only retained update.
    let html = include_str!("../../fixtures/webgl-rendering.html").replace(
        "document.documentElement.setAttribute('data-fixture-ready','true');",
        r#"
        const result = document.getElementById('results');
        if(result.getAttribute('data-passes') !== '18' ||
            result.getAttribute('data-failures') !== '0') throw Error(result.textContent);
        result.textContent += '\nwebgl-passed';
        "#,
    );
    let initial = load_html_document(&session, 379, &html);
    let presentation = completed(&session, initial, "webgl-passed");
    let images = presentation
        .images
        .iter()
        .filter(|image| image.url.starts_with("breeze-internal:canvas:"))
        .collect::<Vec<_>>();
    assert_eq!(images.len(), 2, "native WebGL and Canvas-copy bitmaps");
    for image in &images {
        assert_eq!((image.image.width, image.image.height), (32, 32));
        assert_eq!(image.image.bgra.len(), 32 * 32 * 4);
        assert!(
            image
                .image
                .bgra
                .chunks_exact(4)
                .all(|pixel| pixel[3] == 255)
        );
    }
    assert_eq!(images[0].image.bgra, images[1].image.bgra);
    acknowledge(&session, &presentation);
}
