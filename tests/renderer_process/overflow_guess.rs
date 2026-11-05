//! A long document is laid out at the width it will have once its scrollbar shows.
use super::support::*;
use better_web_browser::engine::DisplayItem;
use better_web_browser::renderer_process::RendererSession;
use better_web_browser::renderer_protocol::{DocumentId, PresentedViewport, RendererPresentation};

const WITH_SCROLLBAR: f32 = 784.0;

/// A red marker filling the layout width, then `content`.
fn page(content: &str) -> String {
    format!(
        "<!doctype html><style>body{{margin:0}}</style>\
         <div style='height:5px;background:rgb(255,0,0)'></div>{content}"
    )
}

fn marker_width(presentation: &RendererPresentation) -> Option<f32> {
    presentation
        .layout
        .items
        .iter()
        .find_map(|item| match item {
            DisplayItem::SolidRect { rect, color, .. } if color.red == 255 && color.green == 0 => {
                Some(rect.width)
            }
            _ => None,
        })
}

fn load(session: &RendererSession, value: u64, html: &str) -> RendererPresentation {
    let document = DocumentId::new(value).unwrap();
    let body = html.as_bytes().to_vec();
    let mut start = document_start(document, body.len());
    start.viewport.overflow_width = WITH_SCROLLBAR;
    session
        .load_document(start, empty_document_state(), body)
        .unwrap();
    wait_for_presentation(session, document, "the marker", |presentation| {
        marker_width(presentation).is_some()
    })
}

#[test]
fn a_long_document_starts_at_its_scrollbar_width_and_keeps_it_when_the_scrollbar_shows() {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut session = RendererSession::launch(options()).expect("launch renderer");
    let first = load(&session, 201, &page(&"<p>paragraph</p>".repeat(1200)));
    assert_eq!(marker_width(&first), Some(WITH_SCROLLBAR));
    assert!(first.layout.content_height > 600.0);
    acknowledge(&session, &first);
    session
        .update_viewport(
            first.document,
            PresentedViewport {
                width: WITH_SCROLLBAR,
                overflow_width: WITH_SCROLLBAR,
                height: 600.0,
                style_width: 800.0,
                dpi: 96,
                prefers_dark_color_scheme: false,
                refresh_rate: Default::default(),
            },
        )
        .unwrap();
    let resized = wait_for_presentation(&session, first.document, "the resize", |presentation| {
        presentation.revision > first.revision
    });
    assert_eq!(marker_width(&resized), Some(WITH_SCROLLBAR));
    session.shutdown().expect("shutdown renderer");
}

#[test]
fn a_long_document_that_fits_is_presented_at_the_full_width() {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut session = RendererSession::launch(options()).expect("launch renderer");
    let fits = load(
        &session,
        202,
        &page(&"<p hidden>paragraph</p>".repeat(1200)),
    );
    assert_eq!(marker_width(&fits), Some(800.0));
    session.shutdown().expect("shutdown renderer");
}

#[test]
fn a_short_document_keeps_the_width_the_browser_reports() {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut session = RendererSession::launch(options()).expect("launch renderer");
    let short = load(&session, 203, &page("<div style='height:5000px'></div>"));
    assert_eq!(marker_width(&short), Some(800.0));
    session.shutdown().expect("shutdown renderer");
}
