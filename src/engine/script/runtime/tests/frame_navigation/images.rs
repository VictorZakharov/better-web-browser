use super::*;
use crate::fetch::RequestDestination;

#[test]
fn child_image_fetch_uses_image_destination_and_repaints_on_decode() {
    let (dom, mut runtime) = start(
        r#"<body><script>
        window.loaded = 0;
        const f = document.createElement('iframe');
        f.srcdoc = '<img src="/tile.png" onload="parent.loaded++">';
        document.body.append(f);
    </script>"#,
    );
    runtime.finish_document_lifecycle();
    let pending = super::styles::requests(&mut runtime);
    assert_eq!(pending.len(), 1);
    let (id, request) = &pending[0];
    assert_eq!(request.url.as_str(), "https://example.com/tile.png");
    assert_eq!(request.destination, RequestDestination::Image);
    assert_eq!(
        request.origin.as_ref().unwrap().serialize(),
        "https://example.com"
    );

    let mut encoded = std::io::Cursor::new(Vec::new());
    image::DynamicImage::new_rgba8(2, 3)
        .write_to(&mut encoded, image::ImageFormat::Png)
        .unwrap();
    let mut image_response = response("https://example.com/tile.png", "");
    image_response.body = crate::fetch::Body::from_bytes(encoded.into_inner());
    let result = runtime.complete_fetch_with_loader(*id, Ok(image_response), None);
    assert!(result.errors.is_empty(), "{:?}", result.errors);
    assert!(result.render_requested, "{:?}", result);
    drain(&mut runtime);
    evaluate(
        &mut runtime,
        &dom,
        "if (loaded !== 1) throw Error('child image load event missing');",
    );
    assert!(runtime.frame_paint_snapshots().iter().any(|frame| {
        frame
            .images
            .get("https://example.com/tile.png")
            .is_some_and(|image| (image.width, image.height) == (2, 3))
    }));
}
