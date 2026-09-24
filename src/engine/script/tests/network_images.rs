//! Decoded detached-image requests and their Canvas consumers.

use super::network::{pending_runtime, test_response};

#[test]
fn detached_image_constructor_loads_through_the_fetch_pipeline() {
    let mut encoded = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
        2,
        1,
        image::Rgba([30, 60, 90, 255]),
    ))
    .write_to(&mut encoded, image::ImageFormat::Png)
    .unwrap();
    let (dom, mut runtime, id) = pending_runtime(
        r#"const image = new Image(320, 180);
        image.onload = async () => {
            await image.decode();
            const canvas = document.createElement('canvas');
            canvas.width = 2; canvas.height = 1;
            const context = canvas.getContext('2d');
            context.drawImage(image, 0, 0);
            const bitmap = await createImageBitmap(image);
            document.querySelector('div').textContent = [
                image instanceof Image,
                image instanceof HTMLImageElement,
                image.complete,
                image.getAttribute('width'),
                image.getAttribute('height'),
                image.naturalWidth,
                image.naturalHeight,
                image.currentSrc,
                bitmap.width,
                [...context.getImageData(0, 0, 1, 1).data].join(',')
            ].join('|');
        };
        image.onerror = () => document.querySelector('div').textContent = 'error';
        image.src = '/thumbnail.jpg';"#,
    );
    let outcome =
        runtime.complete_fetch_with_loader(id, Ok(test_response(encoded.get_ref())), None);

    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("div").next().unwrap().text_content(),
        "true|true|true|320|180|2|1|https://example.com/thumbnail.jpg|2|30,60,90,255"
    );
}

#[test]
fn detached_image_rejects_undecodable_http_bodies() {
    let (dom, mut runtime, id) = pending_runtime(
        r#"const image = document.createElement('img');
        image.onload = () => document.querySelector('div').textContent = 'incorrect load';
        image.onerror = () => image.decode().then(
            () => document.querySelector('div').textContent = 'incorrect decode',
            error => document.querySelector('div').textContent =
                [image.complete, image.naturalWidth, error.name].join('|'));
        image.setAttribute('src', '/broken.png');"#,
    );
    let outcome = runtime.complete_fetch_with_loader(id, Ok(test_response(b"not an image")), None);
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("div").next().unwrap().text_content(),
        "true|0|EncodingError"
    );
}

#[test]
fn detached_svg_image_reports_decoded_natural_dimensions() {
    let (dom, mut runtime, id) = pending_runtime(
        r#"const image = new Image();
        image.onload = () => document.querySelector('div').textContent =
            [image.width, image.height, image.naturalWidth, image.naturalHeight].join('|');
        image.onerror = () => document.querySelector('div').textContent = 'error';
        image.src = '/icon.svg';"#,
    );
    let body = br#"<svg xmlns="http://www.w3.org/2000/svg" width="42" height="42"></svg>"#;
    let outcome = runtime.complete_fetch_with_loader(id, Ok(test_response(body)), None);
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("div").next().unwrap().text_content(),
        "42|42|42|42"
    );
}
