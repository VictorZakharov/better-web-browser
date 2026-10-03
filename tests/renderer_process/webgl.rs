//! Standards fixtures run inside the hidden AppContainer, not a standalone GL process.
use super::support::*;
use better_web_browser::engine::DisplayItem;
use better_web_browser::renderer_process::{RendererEvent, RendererSession, RendererState};
use better_web_browser::renderer_protocol::RendererPresentation;
use std::time::{Duration, Instant};

fn completed_page(session: &RendererSession, html: &str) -> RendererPresentation {
    let initial = load_html_document(session, 802, html);
    let document = initial.document;
    assert!(
        initial.runtime.errors.is_empty(),
        "{:?}",
        initial.runtime.errors
    );
    assert!(!initial.title.starts_with("failed:"), "{}", initial.title);
    if initial.title == "webgl passed" {
        return initial;
    }
    let mut next = initial.next_timer_micros;
    acknowledge(session, &initial);
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        assert!(
            Instant::now() < deadline,
            "WebGL lifecycle did not complete"
        );
        session
            .advance_time(
                document,
                Duration::from_micros(next.unwrap_or(0).saturating_add(2_000)),
                64,
            )
            .unwrap();
        match session.wait_for_event(Duration::from_secs(3)).unwrap() {
            RendererEvent::Presentation(page) if page.document == document => {
                assert!(page.runtime.errors.is_empty(), "{:?}", page.runtime.errors);
                assert!(!page.title.starts_with("failed:"), "{}", page.title);
                if page.title == "webgl passed" {
                    return *page;
                }
                next = page.next_timer_micros;
                acknowledge(session, &page);
            }
            RendererEvent::RuntimeUpdate(update) if update.document == document => {
                assert!(
                    update.runtime.errors.is_empty(),
                    "{:?}",
                    update.runtime.errors
                );
                next = update.next_timer_micros;
            }
            RendererEvent::Diagnostic { .. } => {}
            event => panic!("unexpected WebGL lifecycle event: {event:?}"),
        }
    }
}

#[test]
fn native_webgl_restore_instancing_and_canvas_pixels_cross_the_containment_boundary() {
    let _serial = SERIAL.lock().unwrap_or_else(|error| error.into_inner());
    let mut session =
        RendererSession::launch(options()).expect("launch hidden contained WebGL renderer");
    let page = completed_page(&session, include_str!("../webgl/lifecycle-instancing.html"));
    let url = page
        .layout
        .items
        .iter()
        .find_map(|item| match item {
            DisplayItem::Image { url, .. } if url.starts_with("breeze-internal:canvas:") => {
                Some(url)
            }
            _ => None,
        })
        .expect("restored canvas participates in native presentation");
    let image = page
        .images
        .iter()
        .find(|image| &image.url == url)
        .expect("restored native canvas image transported");
    let green = image
        .image
        .bgra
        .chunks_exact(4)
        .filter(|pixel| pixel[0] == 0 && pixel[1] == 255 && pixel[2] == 0 && pixel[3] == 255)
        .count();
    assert!(green >= 16, "actual instanced pixels missing: {green}");
    session
        .ping(Duration::from_secs(3))
        .expect("renderer protocol survived native compilation and context teardown");
    assert_eq!(session.snapshot().state, RendererState::Running);
    session.shutdown().expect("clean WebGL renderer shutdown");
}
