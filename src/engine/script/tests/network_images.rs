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

#[test]
fn broken_image_draw_and_pattern_throw_after_argument_conversion() {
    let (dom, mut runtime, id) = pending_runtime(
        r#"const image = new Image();
        image.onerror = () => {
            const canvas = document.createElement('canvas'); canvas.width=canvas.height=32;
            const context = canvas.getContext('2d');
            context.fillStyle='red';context.fillRect(0,0,32,32);
            context.globalCompositeOperation='copy';context.shadowColor='blue';context.shadowOffsetX=2;
            const results=[];
            const record = fn => { try { fn();results.push('none'); } catch(error) {results.push(error.name);} };
            record(()=>context.drawImage(image,NaN,0));
            record(()=>context.drawImage(image,0,0));
            record(()=>context.createPattern(image,'invalid'));
            record(()=>context.createPattern(image,Symbol()));
            let conversions=0;
            record(()=>context.createPattern(image,{toString(){conversions++;return 'invalid';}}));
            results.push(conversions,[...context.getImageData(0,0,1,1).data].join(','));
            document.querySelector('div').textContent=results.join('|');
        };
        image.src='/broken.png';"#,
    );
    let outcome = runtime.complete_fetch_with_loader(id, Ok(test_response(b"broken")), None);
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("div").next().unwrap().text_content(),
        "none|InvalidStateError|InvalidStateError|TypeError|InvalidStateError|1|255,0,0,255"
    );
}

#[test]
fn decoded_image_pixels_do_not_depend_on_author_accessors_or_prototypes() {
    let mut encoded = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
        2,
        1,
        image::Rgba([30, 60, 90, 255]),
    ))
    .write_to(&mut encoded, image::ImageFormat::Png)
    .unwrap();
    let (dom, mut runtime, id) = pending_runtime(
        r#"const image = new Image();
        image.onload = async () => {
            for(const name of ['src','srcset','complete','naturalWidth','naturalHeight','getAttribute','hasAttribute'])
                Object.defineProperty(image,name,{get(){throw Error('author accessor '+name);}});
            Object.setPrototypeOf(image,null);
            const canvas=document.createElement('canvas');canvas.width=canvas.height=16;
            const context=canvas.getContext('2d');
            context.drawImage(image,0,0);
            const drawn=[...context.getImageData(0,0,1,1).data].join(',');
            const pattern=context.createPattern(image,'repeat');
            context.fillStyle=pattern;context.fillRect(0,0,16,16);
            const tiled=[...context.getImageData(3,3,1,1).data].join(',');
            const bitmap=await createImageBitmap(image);
            document.querySelector('div').textContent=[drawn,tiled,bitmap.width,bitmap.height].join('|');
        };
        image.src='/private.png';"#,
    );
    let outcome =
        runtime.complete_fetch_with_loader(id, Ok(test_response(encoded.get_ref())), None);
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("div").next().unwrap().text_content(),
        "30,60,90,255|30,60,90,255|2|1"
    );
}
