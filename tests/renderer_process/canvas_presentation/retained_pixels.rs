//! Same-size pixels can reuse paint; DOM/resize/first publication cannot.
use super::*;
use better_web_browser::engine::RectF;
#[path = "retained_pixels/unchanged_styles.rs"]
mod unchanged_styles;

const RETAINED: &str = "Canvas pixel checkpoint reused retained style and layout";

fn image_rect(presentation: &RendererPresentation) -> RectF {
    presentation
        .layout
        .items
        .iter()
        .find_map(|item| match item {
            DisplayItem::Image { url, rect, .. } if url.starts_with("breeze-internal:canvas:") => {
                Some(*rect)
            }
            _ => None,
        })
        .expect("Canvas image item")
}

fn next_canvas_pixels(
    session: &RendererSession,
    previous: &RendererPresentation,
    expected: &[u8],
) -> RendererPresentation {
    let mut timer = previous.next_timer_micros;
    for _ in 0..16 {
        session
            .advance_time(
                previous.document,
                Duration::from_micros(timer.expect("author timer").saturating_add(1000)),
                64,
            )
            .unwrap();
        loop {
            match session.wait_for_event(Duration::from_secs(5)).unwrap() {
                RendererEvent::Presentation(presentation)
                    if presentation.document == previous.document =>
                {
                    assert!(
                        presentation.runtime.errors.is_empty(),
                        "{:?}",
                        presentation.runtime.errors
                    );
                    timer = presentation.next_timer_micros;
                    if presentation.images.iter().any(|image| {
                        image.url.starts_with("breeze-internal:canvas:")
                            && &*image.image.bgra == expected
                    }) {
                        return *presentation;
                    }
                    acknowledge(session, &presentation);
                    break;
                }
                RendererEvent::RuntimeUpdate(update) if update.document == previous.document => {
                    assert!(
                        update.runtime.errors.is_empty(),
                        "{:?}",
                        update.runtime.errors
                    );
                    timer = update.next_timer_micros;
                    break;
                }
                RendererEvent::Diagnostic { .. } | RendererEvent::TextSelectionUpdate(_) => {}
                event => panic!("unexpected Canvas event: {event:?}"),
            }
        }
    }
    panic!("Canvas did not publish the expected pixels");
}

fn reused(presentation: &RendererPresentation) -> bool {
    presentation
        .runtime
        .diagnostics
        .iter()
        .any(|entry| entry == RETAINED)
}

#[test]
fn retained_canvas_pixels_preserve_geometry_and_dom_work_overrides_the_shortcut() {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut session = RendererSession::launch(options()).expect("launch hidden renderer");
    let initial = load_html_document_with_selectors(
        &session,
        348,
        r#"
        <!doctype html><style>body{margin:0}canvas{display:block}</style>
        <canvas width=2 height=1></canvas><p id=label>before</p><script>
            const canvas=document.querySelector('canvas'), gl=canvas.getContext('2d');
            const paint=(color)=>{gl.fillStyle=color;gl.fillRect(0,0,canvas.width,canvas.height)};
            paint('red');
            setTimeout(()=>paint('blue'),10000);
            setTimeout(()=>{canvas.style.marginLeft='17px';paint('green')},20000);
            setTimeout(()=>{canvas.width=3;paint('yellow')},30000);
            setTimeout(()=>{document.querySelector('#label').textContent='after';paint('white')},40000);
        </script>
    "#,
        vec!["canvas".into(), "#label".into()],
    );
    acknowledge(&session, &initial);
    let blue = next_canvas_pixels(&session, &initial, &[255, 0, 0, 255, 255, 0, 0, 255]);
    assert!(reused(&blue), "{:?}", blue.runtime.diagnostics);
    assert_eq!(blue.style.recomputed_styles, 0);
    assert_eq!(image_rect(&blue).x, 0.0);
    acknowledge(&session, &blue);

    let green = next_canvas_pixels(&session, &blue, &[0, 128, 0, 255, 0, 128, 0, 255]);
    assert!(
        !reused(&green),
        "CSS mutations must not inherit the pixel-only request"
    );
    assert_eq!(image_rect(&green).x, 17.0);
    acknowledge(&session, &green);

    let yellow = next_canvas_pixels(&session, &green, &[0, 255, 255, 255].repeat(3));
    assert!(
        !reused(&yellow),
        "resized bitmaps require new natural geometry"
    );
    assert_eq!(image_rect(&yellow).width, 3.0);
    assert!(
        yellow
            .retired_image_keys
            .iter()
            .any(|key| key.ends_with(":2x1"))
    );
    acknowledge(&session, &yellow);

    let white = next_canvas_pixels(&session, &yellow, &[255, 255, 255, 255].repeat(3));
    assert!(!reused(&white), "text mutations still require layout");
    assert!(
        white
            .layout
            .items
            .iter()
            .any(|item| matches!(item, DisplayItem::Text { text, .. } if text == "after"))
    );
    session.shutdown().unwrap();
}

#[test]
fn first_canvas_bitmap_uses_normal_layout_even_when_only_pixels_changed() {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut session = RendererSession::launch(options()).expect("launch hidden renderer");
    let initial = load_html_document_with_selectors(
        &session,
        349,
        r#"
        <!doctype html><canvas width=2 height=1></canvas><script>
            setTimeout(()=>{const c=document.querySelector('canvas').getContext('2d');c.fillStyle='blue';c.fillRect(0,0,2,1)},10000);
        </script>
    "#,
        vec!["canvas".into()],
    );
    acknowledge(&session, &initial);
    let blue = next_canvas_pixels(&session, &initial, &[255, 0, 0, 255, 255, 0, 0, 255]);
    assert!(
        !reused(&blue),
        "a first bitmap has no retained image item yet"
    );
    assert_eq!(image_rect(&blue).width, 2.0);
    session.shutdown().unwrap();
}
