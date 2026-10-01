//! Real modern-image pixels must cross the contained renderer's presentation IPC.
use super::super::support::*;
use better_web_browser::engine::DisplayItem;
use better_web_browser::renderer_process::{RendererEvent, RendererSession};
use better_web_browser::renderer_protocol::RendererPresentation;
use std::time::{Duration, Instant};

const RGB_AVIF: &str = include_str!("../../image-fixtures/rgb-lossless.avif.base64");
const RGBA_AVIF: &str = include_str!("../../image-fixtures/rgba-lossless.avif.base64");
const RGBA_JXL: &str = include_str!("../../image-fixtures/rgba-lossless.jxl.base64");
const OPAQUE: [u8; 24] = [
    0, 0, 255, 255, 0, 255, 0, 255, 255, 0, 0, 255, 255, 255, 255, 255, 0, 0, 0, 255, 32, 64, 128,
    255,
];
const ALPHA: [u8; 24] = [
    0, 0, 255, 255, 0, 128, 0, 128, 64, 0, 0, 64, 0, 0, 0, 0, 0, 0, 0, 255, 24, 48, 96, 192,
];

fn has_text(presentation: &RendererPresentation, wanted: &str) -> bool {
    presentation
        .layout
        .items
        .iter()
        .any(|item| matches!(item, DisplayItem::Text { text, .. } if text.contains(wanted)))
}

pub(super) fn completed(
    session: &RendererSession,
    mut presentation: RendererPresentation,
    marker: &str,
) -> RendererPresentation {
    let deadline = Instant::now() + Duration::from_secs(12);
    for _ in 0..12 {
        assert!(
            presentation.runtime.errors.is_empty(),
            "{:?}",
            presentation.runtime.errors
        );
        if has_text(&presentation, marker) {
            return presentation;
        }
        acknowledge(session, &presentation);
        session
            .advance_time(presentation.document, Duration::from_millis(10), 64)
            .unwrap();
        loop {
            assert!(
                Instant::now() < deadline,
                "no modern-image presentation: {marker}"
            );
            match session.wait_for_event(Duration::from_secs(3)).unwrap() {
                RendererEvent::Presentation(next) if next.document == presentation.document => {
                    presentation = *next;
                    break;
                }
                RendererEvent::RuntimeUpdate(update)
                    if update.document == presentation.document =>
                {
                    assert!(
                        update.runtime.errors.is_empty(),
                        "{:?}",
                        update.runtime.errors
                    );
                    session
                        .advance_time(update.document, Duration::from_millis(10), 64)
                        .unwrap();
                }
                RendererEvent::Diagnostic { .. } => {}
                event => panic!("unexpected modern-image checkpoint: {event:?}"),
            }
        }
    }
    panic!("modern-image promise never produced {marker}");
}

fn bitmap_document(encoded: &str, context: &str) -> String {
    let base64: String = encoded.chars().filter(|c| !c.is_whitespace()).collect();
    format!(
        r#"<!doctype html><style>body{{margin:0}}</style>
        <canvas width=9 height=8></canvas><p id=status>pending</p><script>
        (async () => {{
            const bytes = Uint8Array.from(atob('{base64}'), c => c.charCodeAt(0));
            const bitmap = await createImageBitmap(new Blob([bytes]), {{premultiplyAlpha:'none'}});
            const canvas = document.querySelector('canvas');
            if ('{context}' === 'bitmaprenderer') {{
                canvas.getContext('bitmaprenderer').transferFromImageBitmap(bitmap);
                if (canvas.width !== 9 || canvas.height !== 8 || bitmap.width !== 0)
                    throw new Error('bitmap ownership or content attributes changed');
            }} else {{
                canvas.width = bitmap.width; canvas.height = bitmap.height;
                canvas.getContext('2d').drawImage(bitmap, 0, 0); bitmap.close();
            }}
            document.querySelector('#status').textContent = 'decoded';
        }})().catch(error => {{ document.querySelector('#status').textContent = 'failed:' + error; }});
        </script>"#
    )
}

#[test]
fn modern_image_pixels_cross_the_contained_renderer_without_double_premultiplication() {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    for (index, (encoded, expected)) in [(RGB_AVIF, OPAQUE), (RGBA_AVIF, ALPHA), (RGBA_JXL, ALPHA)]
        .into_iter()
        .enumerate()
    {
        for (mode, context) in ["2d", "bitmaprenderer"].into_iter().enumerate() {
            let session = RendererSession::launch(options()).expect("launch hidden renderer");
            let initial = load_html_document(
                &session,
                281 + (index * 2 + mode) as u64,
                &bitmap_document(encoded, context),
            );
            let presentation = completed(&session, initial, "decoded");
            let image = presentation
                .images
                .iter()
                .find(|image| image.url.starts_with("breeze-internal:canvas:"))
                .expect("decoded canvas pixels crossed presentation IPC");
            assert_eq!((image.image.width, image.image.height), (3, 2), "{context}");
            assert_eq!(
                image.image.bgra.as_ref(),
                expected,
                "fixture {index}, context {context}"
            );
            acknowledge(&session, &presentation);
        }
    }
}

#[test]
fn malformed_modern_images_reject_without_stopping_the_contained_document() {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let session = RendererSession::launch(options()).expect("launch hidden renderer");
    let base64: String = RGBA_JXL.chars().filter(|c| !c.is_whitespace()).collect();
    let html = format!(
        r#"<!doctype html><canvas width=1 height=1></canvas>
        <p id=status>pending</p><script>
        (async () => {{
            const complete = Uint8Array.from(atob('{base64}'), c => c.charCodeAt(0));
            const malformed = [new Uint8Array([255,10]), complete.slice(0,12),
                new Uint8Array([0,0,0,16,102,116,121,112,97,118,105,102,0,0,0,0])];
            let rejected = 0;
            for (const bytes of malformed) {{
                try {{ await createImageBitmap(new Blob([bytes])); }}
                catch (error) {{ if (error.name === 'InvalidStateError') rejected++; }}
            }}
            const context = document.querySelector('canvas').getContext('2d');
            context.fillStyle = '#00ff00'; context.fillRect(0,0,1,1);
            document.querySelector('#status').textContent = 'rejected:' + rejected;
        }})().catch(error => {{ document.querySelector('#status').textContent = 'failed:' + error; }});
        </script>"#
    );
    let initial = load_html_document(&session, 289, &html);
    let presentation = completed(&session, initial, "rejected:3");
    let image = presentation
        .images
        .iter()
        .find(|image| image.url.starts_with("breeze-internal:canvas:"))
        .expect("renderer still presents after decoder errors");
    assert_eq!((image.image.width, image.image.height), (1, 1));
    assert_eq!(image.image.bgra.as_ref(), [0, 255, 0, 255]);
    acknowledge(&session, &presentation);
}
