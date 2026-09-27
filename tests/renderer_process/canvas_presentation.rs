use super::support::*;
use better_web_browser::engine::DisplayItem;
use better_web_browser::renderer_process::{RendererEvent, RendererSession};
use better_web_browser::renderer_protocol::RendererPresentation;
use std::time::Duration;

fn canvas_image(presentation: &RendererPresentation) -> (&str, &[u8]) {
    let url = presentation
        .layout
        .items
        .iter()
        .find_map(|item| match item {
            DisplayItem::Image { url, .. } if url.starts_with("breeze-internal:canvas:") => {
                Some(url.as_str())
            }
            _ => None,
        })
        .expect("Canvas produced an image display item");
    let image = presentation
        .images
        .iter()
        .find(|image| image.url == url)
        .expect("Canvas bitmap was sent with the presentation");
    assert_eq!((image.image.width, image.image.height), (2, 1));
    (url, &image.image.bgra)
}

#[test]
fn html_canvas_sends_real_pixels_and_replaces_them_at_the_next_paint_checkpoint() {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut session = RendererSession::launch(options()).expect("launch hidden renderer");
    let initial = load_html_document(
        &session,
        241,
        r#"<!doctype html><style>body{margin:0}</style>
        <canvas width=2 height=1>fallback</canvas><script>
            const canvas = document.querySelector('canvas');
            const context = canvas.getContext('2d');
            const first = context.createImageData(2, 1);
            first.data.set([200, 100, 50, 128, 0, 0, 0, 0]);
            context.putImageData(first, 0, 0);
            setTimeout(() => {
                context.clearRect(0, 0, 2, 1);
                context.fillStyle = '#0000ff';
                context.fillRect(1, 0, 1, 1);
            }, 10000);
            setTimeout(() => canvas.remove(), 20000);
            setTimeout(() => document.body.append(canvas), 30000);
        </script>"#,
    );
    let (key, pixels) = canvas_image(&initial);
    let key = key.to_string();
    assert_eq!(pixels, [25, 50, 100, 128, 0, 0, 0, 0]);
    assert!(
        initial.runtime.errors.is_empty(),
        "{:?}",
        initial.runtime.errors
    );
    acknowledge(&session, &initial);

    // The parser's first presentation can precede the zero-delay load task.
    // Drain scheduled checkpoints until the author timer replaces the bitmap.
    let mut next_timer = initial.next_timer_micros;
    let mut updated = None;
    for _ in 0..8 {
        let delay = next_timer.expect("Canvas author timer is still scheduled");
        session
            .advance_time(
                initial.document,
                Duration::from_micros(delay.saturating_add(1_000)),
                64,
            )
            .unwrap();
        let event = session.wait_for_event(Duration::from_secs(3)).unwrap();
        match event {
            RendererEvent::Presentation(presentation)
                if presentation.document == initial.document =>
            {
                next_timer = presentation.next_timer_micros;
                if presentation.images.iter().any(|image| image.url == key) {
                    updated = Some(*presentation);
                    break;
                }
                acknowledge(&session, &presentation);
            }
            RendererEvent::RuntimeUpdate(update) if update.document == initial.document => {
                assert!(
                    update.runtime.errors.is_empty(),
                    "{:?}",
                    update.runtime.errors
                );
                next_timer = update.next_timer_micros;
            }
            event => panic!("unexpected event awaiting Canvas paint: {event:?}"),
        }
    }
    let updated = updated.expect("timer paint did not replace the Canvas bitmap");
    let (updated_key, pixels) = canvas_image(&updated);
    assert_eq!(updated_key, key);
    assert_eq!(pixels, [0, 0, 0, 0, 255, 0, 0, 255]);
    acknowledge(&session, &updated);

    let mut next_timer = updated.next_timer_micros;
    let mut retired = None;
    for _ in 0..8 {
        let delay = next_timer.expect("Canvas removal timer is still scheduled");
        session
            .advance_time(
                initial.document,
                Duration::from_micros(delay.saturating_add(1_000)),
                64,
            )
            .unwrap();
        match session.wait_for_event(Duration::from_secs(3)).unwrap() {
            RendererEvent::Presentation(presentation)
                if presentation.document == initial.document =>
            {
                next_timer = presentation.next_timer_micros;
                if presentation.retired_image_keys.contains(&key) {
                    assert!(!presentation.images.iter().any(|image| image.url == key));
                    retired = Some(*presentation);
                    break;
                }
                acknowledge(&session, &presentation);
            }
            RendererEvent::RuntimeUpdate(update) if update.document == initial.document => {
                assert!(
                    update.runtime.errors.is_empty(),
                    "{:?}",
                    update.runtime.errors
                );
                next_timer = update.next_timer_micros;
            }
            event => panic!("unexpected event awaiting Canvas retirement: {event:?}"),
        }
    }
    let retired = retired.expect("detached Canvas bitmap was not retired");
    acknowledge(&session, &retired);
    let mut next_timer = retired.next_timer_micros;
    let mut restored = None;
    for _ in 0..8 {
        let delay = next_timer.expect("Canvas reattachment timer is still scheduled");
        session
            .advance_time(
                initial.document,
                Duration::from_micros(delay.saturating_add(1_000)),
                64,
            )
            .unwrap();
        match session.wait_for_event(Duration::from_secs(3)).unwrap() {
            RendererEvent::Presentation(presentation)
                if presentation.document == initial.document =>
            {
                next_timer = presentation.next_timer_micros;
                if presentation.images.iter().any(|image| image.url == key) {
                    restored = Some(*presentation);
                    break;
                }
                acknowledge(&session, &presentation);
            }
            RendererEvent::RuntimeUpdate(update) if update.document == initial.document => {
                assert!(
                    update.runtime.errors.is_empty(),
                    "{:?}",
                    update.runtime.errors
                );
                next_timer = update.next_timer_micros;
            }
            event => panic!("unexpected event awaiting Canvas reattachment: {event:?}"),
        }
    }
    let restored = restored.expect("reattached Canvas was not re-published without repaint");
    let (restored_key, pixels) = canvas_image(&restored);
    assert_eq!(restored_key, key);
    assert_eq!(pixels, [0, 0, 0, 0, 255, 0, 0, 255]);
    session.shutdown().expect("shutdown renderer");
}
