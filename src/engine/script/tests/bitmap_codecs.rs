//! The actual Blob, Image element and dedicated-worker routes use the same codecs.
use super::*;
use crate::engine::image_decode::{self, DecodeLimits, DecodeOptions, fixtures};
use std::sync::Arc;

fn blob_source(encoded: &str) -> String {
    format!(
        "new Blob([Uint8Array.from(atob('{}'),c=>c.charCodeAt(0))],{{type:'application/octet-stream'}})",
        encoded.split_whitespace().collect::<String>()
    )
}

#[test]
fn window_imagebitmap_blob_decoding_reads_real_modern_pixels_not_mime_claims() {
    for fixture in fixtures::MODERN {
        let expected = image_decode::decode(
            &fixtures::bytes(fixture.encoded),
            DecodeLimits::CANVAS,
            DecodeOptions::default(),
        )
        .unwrap();
        let body = format!(
            r#"<body><output>pending</output><script>
            createImageBitmap({}).then(bitmap=>{{
                const context=new OffscreenCanvas(bitmap.width,bitmap.height).getContext('2d');
                context.drawImage(bitmap,0,0);
                const data=[...context.getImageData(0,0,bitmap.width,bitmap.height).data];
                document.querySelector('output').textContent=bitmap.width+'x'+bitmap.height+'|'+data.join(',');
            }},error=>{{document.querySelector('output').textContent=error.name;}});
        </script></body>"#,
            blob_source(fixture.encoded)
        );
        let (dom, outcome) = execute_html(&body);
        assert!(
            outcome.errors.is_empty(),
            "{}: {:?}",
            fixture.name,
            outcome.errors
        );
        // Canvas source-over normalizes hidden RGB at zero coverage.
        let mut pixels = expected.rgba;
        for pixel in pixels.chunks_exact_mut(4) {
            if pixel[3] == 0 {
                pixel[..3].fill(0);
            }
        }
        let expected = format!(
            "3x2|{}",
            pixels
                .iter()
                .map(u8::to_string)
                .collect::<Vec<_>>()
                .join(",")
        );
        assert_eq!(
            dom.elements_named("output").next().unwrap().text_content(),
            expected,
            "{}",
            fixture.name
        );
    }
}

#[test]
fn blob_author_methods_are_not_invoked_by_internal_bitmap_byte_consumption() {
    let fixture = fixtures::MODERN
        .iter()
        .find(|fixture| fixture.name == "rgb-lossless.avif")
        .unwrap();
    let body = format!(
        r#"<body><output>pending</output><script>
        const blob={};
        for(const member of ['bytes','arrayBuffer','stream','text','size','type'])
            Object.defineProperty(blob,member,{{get(){{throw Error('author getter '+member);}}}});
        createImageBitmap(blob).then(bitmap=>{{
            document.querySelector('output').textContent=bitmap.width+'x'+bitmap.height;
        }},error=>{{document.querySelector('output').textContent=error.name+':'+error.message;}});
    </script></body>"#,
        blob_source(fixture.encoded)
    );
    let (dom, outcome) = execute_html(&body);
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "3x2"
    );
}

#[test]
fn file_subclasses_decode_owned_bytes_without_consulting_overridden_bytes_methods() {
    let encoded = fixtures::MODERN[7]
        .encoded
        .split_whitespace()
        .collect::<String>();
    let html = format!(
        r#"<body><output>pending</output><script>
        class MyFile extends File {{bytes(){{throw Error('author bytes');}}}}
        const source=new MyFile([Uint8Array.from(atob('{encoded}'),c=>c.charCodeAt(0))],'pixels.jxl');
        createImageBitmap(source).then(bitmap=>{{document.querySelector('output').textContent=bitmap.width;}},
            error=>{{document.querySelector('output').textContent=error.name;}});
    </script></body>"#
    );
    let (dom, outcome) = execute_html(&html);
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "3"
    );
}

#[test]
fn worker_imagebitmap_decodes_avif_and_jxl_and_transfers_real_pixels() {
    for fixture in [
        fixtures::MODERN
            .iter()
            .find(|fixture| fixture.name == "rgb-lossless.avif")
            .unwrap(),
        fixtures::MODERN
            .iter()
            .find(|fixture| fixture.name == "rgba-lossless.jxl")
            .unwrap(),
    ] {
        let source = format!(
            r#"
            if('__imageBitmapBlobSnapshot' in globalThis || '__bindImageBitmapBlob' in globalThis)
                throw Error('internal Blob capability leaked');
            createImageBitmap({}).then(bitmap=>{{
                const target=new OffscreenCanvas(3,2).getContext('2d');target.drawImage(bitmap,0,0);
                postMessage({{width:bitmap.width,height:bitmap.height,pixels:[...target.getImageData(0,0,3,2).data]}});
            }},error=>{{throw error;}});
        "#,
            blob_source(fixture.encoded)
        );
        let (_, outcome) = WorkerRuntime::start(
            "https://example.com/bitmap.js",
            &source,
            "image-codec",
            ScriptKind::Classic,
            Arc::new(|_, _| Err("unexpected import".into())),
        );
        assert!(
            outcome.errors.is_empty(),
            "{}: {:?}",
            fixture.name,
            outcome.errors
        );
        assert_eq!(outcome.messages.len(), 1, "{}: {:?}", fixture.name, outcome);
        assert!(outcome.messages[0].contains("255"));
        assert!(outcome.messages[0].contains("\"width\",3"));
        assert!(outcome.messages[0].contains("\"height\",2"));
    }
}

#[test]
fn declared_image_mime_does_not_turn_invalid_encoded_bytes_into_supported_pixels() {
    let (dom, outcome) = execute_html(
        r#"<body><output>pending</output><script>
        Promise.all(['image/avif','image/jxl','image/png','image/svg+xml'].map(type=>
            createImageBitmap(new Blob(['not an image'],{type})).then(()=>false,error=>error.name)))
            .then(values=>{document.querySelector('output').textContent=values.join(',');});
    </script></body>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "InvalidStateError,InvalidStateError,InvalidStateError,InvalidStateError"
    );
}

#[test]
fn svg_bitmap_dimension_budget_is_checked_before_pixmap_allocation() {
    let (dom, outcome) = execute_html(
        r#"<body><output>pending</output><script>
        createImageBitmap(new Blob(['<svg xmlns="http://www.w3.org/2000/svg" width="8192" height="8192"/>'],
            {type:'image/svg+xml'})).then(()=>{document.querySelector('output').textContent='accepted';},
            error=>{document.querySelector('output').textContent=error.name;});
    </script></body>"#,
    );
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "InvalidStateError"
    );
}

#[test]
fn independent_blob_decodes_can_be_closed_without_invalidating_other_outputs() {
    let fixture = &fixtures::MODERN[7];
    let html = format!(
        r#"<body><output>pending</output><script>
        const source={};
        Promise.all([createImageBitmap(source),createImageBitmap(source)]).then(([a,b])=>{{
            a.close();document.querySelector('output').textContent=[a.width,b.width,b.height].join(',');
        }});
    </script></body>"#,
        blob_source(fixture.encoded)
    );
    let (dom, outcome) = execute_html(&html);
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    assert_eq!(
        dom.elements_named("output").next().unwrap().text_content(),
        "0,3,2"
    );
}
