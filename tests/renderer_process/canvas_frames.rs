use super::support::*;
use better_web_browser::engine::DisplayItem;
use better_web_browser::renderer_process::{RendererEvent, RendererSession};
use better_web_browser::renderer_protocol::RendererPresentation;
use std::collections::HashSet;
use std::time::Duration;

fn child_canvas_image(presentation: &RendererPresentation) -> Option<(&str, &[u8])> {
    let url = presentation
        .layout
        .items
        .iter()
        .find_map(|item| match item {
            DisplayItem::Image { url, .. } if url.starts_with("breeze-internal:canvas:") => {
                Some(url.as_str())
            }
            _ => None,
        })?;
    let image = presentation.images.iter().find(|image| image.url == url)?;
    Some((url, &image.image.bgra))
}

#[test]
fn srcdoc_canvas_pixels_cross_the_child_frame_paint_boundary() {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut session = RendererSession::launch(options()).expect("launch hidden renderer");
    let initial = load_html_document(
        &session,
        243,
        r#"<!doctype html><style>body{margin:0}</style><p>waiting</p><script>
            const frame = document.createElement('iframe');
            frame.width = '200';
            frame.height = '100';
            frame.srcdoc = '<canvas width=2 height=1></canvas><script>' +
                'const canvas = document.querySelector("canvas");' +
                'const context = canvas.getContext("2d");' +
                'const data = context.createImageData(2,1);' +
                'data.data.set([200,100,50,128,0,0,0,0]);' +
                'context.putImageData(data,0,0);' +
                'setTimeout(() => { context.fillStyle = "blue";' +
                'context.fillRect(1,0,1,1); window.painted = true; }, 10000);' +
                '<\/script>';
            document.body.append(frame);
            setTimeout(() => document.querySelector('p').textContent = 'tick', 1);
            setTimeout(() => {
                frame.style.display = 'none';
                document.querySelector('p').textContent = 'hidden';
            }, 5000);
            setTimeout(() => {
                document.querySelector('p').textContent = frame.contentWindow.painted
                    ? 'visible:painted' : 'visible:late';
                frame.style.display = 'block';
            }, 15000);
            setTimeout(() => frame.remove(), 25000);
        </script>"#,
    );
    let document = initial.document;
    assert!(
        initial.runtime.errors.is_empty(),
        "{:?}",
        initial.runtime.errors
    );
    let mut next_timer = initial.next_timer_micros;
    let mut found =
        child_canvas_image(&initial).map(|(key, pixels)| (key.to_string(), pixels.to_vec()));
    acknowledge(&session, &initial);
    for _ in 0..12 {
        if found.is_some() {
            break;
        }
        session
            .advance_time(
                document,
                Duration::from_micros(next_timer.unwrap_or(0).saturating_add(1_000)),
                64,
            )
            .unwrap();
        match session.wait_for_event(Duration::from_secs(3)).unwrap() {
            RendererEvent::Presentation(presentation) if presentation.document == document => {
                assert!(
                    presentation.runtime.errors.is_empty(),
                    "{:?}",
                    presentation.runtime.errors
                );
                next_timer = presentation.next_timer_micros;
                found = child_canvas_image(&presentation)
                    .map(|(key, pixels)| (key.to_string(), pixels.to_vec()));
                acknowledge(&session, &presentation);
            }
            RendererEvent::RuntimeUpdate(update) if update.document == document => {
                assert!(
                    update.runtime.errors.is_empty(),
                    "{:?}",
                    update.runtime.errors
                );
                next_timer = update.next_timer_micros;
            }
            RendererEvent::Diagnostic { .. } => {}
            event => panic!("unexpected child Canvas event: {event:?}"),
        }
    }
    let (key, pixels) = found.expect("child Canvas produced no presented bitmap");
    assert_eq!(pixels, [25, 50, 100, 128, 0, 0, 0, 0]);

    let mut updated = None;
    let mut saw_hidden = false;
    for _ in 0..12 {
        let delay = next_timer.expect("child Canvas repaint timer is still scheduled");
        session
            .advance_time(
                document,
                Duration::from_micros(delay.saturating_add(1_000)),
                64,
            )
            .unwrap();
        match session.wait_for_event(Duration::from_secs(3)).unwrap() {
            RendererEvent::Presentation(presentation) if presentation.document == document => {
                assert!(
                    presentation.runtime.errors.is_empty(),
                    "{:?}",
                    presentation.runtime.errors
                );
                next_timer = presentation.next_timer_micros;
                let text = presentation
                    .layout
                    .items
                    .iter()
                    .filter_map(|item| match item {
                        DisplayItem::Text { text, .. } => Some(text.as_str()),
                        _ => None,
                    })
                    .collect::<String>();
                if text.contains("hidden") {
                    saw_hidden = true;
                    assert!(!presentation.layout.items.iter().any(|item| matches!(item,
                        DisplayItem::Image { url, .. } if url == &key)));
                }
                if let Some((updated_key, pixels)) = child_canvas_image(&presentation) {
                    assert_eq!(updated_key, key);
                    assert!(
                        saw_hidden,
                        "Canvas repaint occurred before frame was hidden"
                    );
                    assert!(text.contains("visible:painted"), "{text}");
                    updated = Some(pixels.to_vec());
                }
                acknowledge(&session, &presentation);
                if updated.is_some() {
                    break;
                }
            }
            RendererEvent::RuntimeUpdate(update) if update.document == document => {
                assert!(
                    update.runtime.errors.is_empty(),
                    "{:?}",
                    update.runtime.errors
                );
                next_timer = update.next_timer_micros;
            }
            RendererEvent::Diagnostic { .. } => {}
            event => panic!("unexpected child Canvas repaint event: {event:?}"),
        }
    }
    assert_eq!(
        updated.expect("child Canvas repaint was not re-published"),
        [25, 50, 100, 128, 255, 0, 0, 255]
    );
    assert!(saw_hidden, "iframe never presented hidden state");

    let mut retired = false;
    for _ in 0..12 {
        let delay = next_timer.expect("iframe removal timer is still scheduled");
        session
            .advance_time(
                document,
                Duration::from_micros(delay.saturating_add(1_000)),
                64,
            )
            .unwrap();
        match session.wait_for_event(Duration::from_secs(3)).unwrap() {
            RendererEvent::Presentation(presentation) if presentation.document == document => {
                assert!(
                    presentation.runtime.errors.is_empty(),
                    "{:?}",
                    presentation.runtime.errors
                );
                next_timer = presentation.next_timer_micros;
                retired = presentation.retired_image_keys.contains(&key);
                acknowledge(&session, &presentation);
                if retired {
                    break;
                }
            }
            RendererEvent::RuntimeUpdate(update) if update.document == document => {
                assert!(
                    update.runtime.errors.is_empty(),
                    "{:?}",
                    update.runtime.errors
                );
                next_timer = update.next_timer_micros;
            }
            RendererEvent::Diagnostic { .. } => {}
            event => panic!("unexpected child Canvas retirement event: {event:?}"),
        }
    }
    assert!(
        retired,
        "removed child frame did not retire its Canvas bitmap"
    );
    session.shutdown().expect("shutdown renderer");
}

#[test]
fn root_and_child_canvas_bitmaps_are_deferred_within_the_wire_budget() {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut session = RendererSession::launch(options()).expect("launch hidden renderer");
    let initial = load_html_document(
        &session,
        244,
        r#"<!doctype html><style>body{margin:0}canvas{width:1px;height:1px}</style>
            <canvas width=2048 height=2048></canvas><canvas width=2048 height=2048></canvas>
            <iframe width=4 height=4 srcdoc='<style>canvas{width:1px;height:1px}</style>
                <canvas width=2048 height=2048></canvas><canvas width=2048 height=2048></canvas>'></iframe>
            <script>
            const frame = document.querySelector('iframe');
            function paintTogether() {
                const child = frame.contentDocument?.querySelectorAll('canvas');
                if (!child || child.length !== 2) {
                    setTimeout(paintTogether, 1);
                    return;
                }
                for (const canvas of document.querySelectorAll('body > canvas')) {
                    const context = canvas.getContext('2d');
                    context.fillStyle = 'red';
                    context.fillRect(0, 0, 1, 1);
                }
                for (const canvas of child) {
                    const context = canvas.getContext('2d');
                    context.fillStyle = 'blue';
                    context.fillRect(0, 0, 1, 1);
                }
            }
            setTimeout(paintTogether, 1);
            </script>"#,
    );
    let document = initial.document;
    let mut seen = HashSet::new();
    let mut red = 0;
    let mut blue = 0;
    let mut image_presentations = 0;
    let mut deferred_checkpoint = false;
    let mut next_timer = initial.next_timer_micros;
    for step in 0..16 {
        let event = if step == 0 {
            RendererEvent::Presentation(Box::new(initial.clone()))
        } else {
            session
                .advance_time(
                    document,
                    Duration::from_micros(next_timer.unwrap_or(0).saturating_add(1_000)),
                    64,
                )
                .unwrap();
            session.wait_for_event(Duration::from_secs(5)).unwrap()
        };
        match event {
            RendererEvent::Presentation(presentation) if presentation.document == document => {
                assert!(
                    presentation.runtime.errors.is_empty(),
                    "{:?}",
                    presentation.runtime.errors
                );
                next_timer = presentation.next_timer_micros;
                let canvases: Vec<_> = presentation
                    .images
                    .iter()
                    .filter(|image| image.url.starts_with("breeze-internal:canvas:"))
                    .collect();
                if !canvases.is_empty() {
                    image_presentations += 1;
                    assert!(
                        presentation.encode().is_ok(),
                        "Canvas delta exceeded the 64 MiB wire budget"
                    );
                    deferred_checkpoint |= presentation.next_timer_micros == Some(0);
                }
                for image in canvases {
                    if seen.insert(image.url.clone()) {
                        assert_eq!(image.image.bgra.len(), 2048 * 2048 * 4);
                        match &image.image.bgra[..4] {
                            [0, 0, 255, 255] => red += 1,
                            [255, 0, 0, 255] => blue += 1,
                            pixel => panic!("unexpected Canvas pixel: {pixel:?}"),
                        }
                    }
                }
                acknowledge(&session, &presentation);
                if seen.len() == 4 {
                    break;
                }
            }
            RendererEvent::RuntimeUpdate(update) if update.document == document => {
                assert!(
                    update.runtime.errors.is_empty(),
                    "{:?}",
                    update.runtime.errors
                );
                next_timer = update.next_timer_micros;
            }
            RendererEvent::Diagnostic { .. } => {}
            event => panic!("unexpected split Canvas renderer event: {event:?}"),
        }
    }
    assert_eq!(
        (red, blue),
        (2, 2),
        "root and child Canvas pixels were not both delivered"
    );
    assert!(image_presentations >= 2, "Canvas deltas were not split");
    assert!(
        deferred_checkpoint,
        "deferred Canvas work was not scheduled"
    );
    session.shutdown().expect("shutdown renderer");
}
