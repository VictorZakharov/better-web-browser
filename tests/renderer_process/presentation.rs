use super::support::*;
use better_web_browser::engine::DisplayItem;
use better_web_browser::engine::css::Color;
use better_web_browser::renderer_process::{RendererEvent, RendererSession, RendererState};
use better_web_browser::renderer_protocol::{
    FetchResponseHead, FetchResponseResult, FetchResponseType, RendererFetchRequest,
    ResourceDestination, TransferChunk,
};
use better_web_browser::renderer_protocol::{PresentationAcknowledgement, PresentedViewport};
use std::io::Cursor;
use std::time::Duration;

#[test]
fn presentation_bursts_keep_the_newest_revision_without_killing_the_renderer() {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut session = RendererSession::launch(options()).expect("launch renderer");
    let initial = load_inline_document(&session, 93);
    session
        .acknowledge_presentation(PresentationAcknowledgement {
            document: initial.document,
            revision: initial.revision,
            presented: true,
            controls_applied: true,
        })
        .unwrap();

    for step in 1..=4 {
        session
            .update_viewport(
                initial.document,
                PresentedViewport {
                    width: 800.0 + step as f32,
                    height: 600.0,
                    style_width: 800.0 + step as f32,
                    dpi: 96,
                    prefers_dark_color_scheme: false,
                },
            )
            .unwrap();
        // The Pong follows the corresponding presentation on the renderer output pipe. Waiting
        // here makes the browser-side backlog deterministic without consuming its events.
        session
            .ping(Duration::from_secs(2))
            .expect("renderer survived presentation burst");
    }

    let mut presentations = Vec::new();
    while let Some(event) = session.try_event().unwrap() {
        match event {
            RendererEvent::Presentation(presentation) => presentations.push(presentation),
            RendererEvent::Diagnostic { .. } => {}
            event => panic!("unexpected renderer event after presentation burst: {event:?}"),
        }
    }
    assert_eq!(presentations.len(), 1);
    let newest = presentations.pop().unwrap();
    assert_eq!(newest.revision, initial.revision + 4);
    session
        .acknowledge_presentation(PresentationAcknowledgement {
            document: newest.document,
            revision: newest.revision,
            presented: true,
            controls_applied: true,
        })
        .unwrap();
    session
        .ping(Duration::from_secs(1))
        .expect("renderer accepts acknowledgement that skips coalesced revisions");
    assert_eq!(session.snapshot().state, RendererState::Running);
    session.shutdown().expect("shutdown renderer");
}

#[test]
fn console_only_timer_uses_a_runtime_update_without_repainting() {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut session = RendererSession::launch(options()).expect("launch renderer");
    let initial = load_html_document(
        &session,
        94,
        r#"<!doctype html><title>quiet timer</title>
        <p>stable pixels</p>
        <script>setTimeout(() => console.log('timer completed'), 1600);</script>"#,
    );
    session
        .acknowledge_presentation(PresentationAcknowledgement {
            document: initial.document,
            revision: initial.revision,
            presented: true,
            controls_applied: true,
        })
        .unwrap();
    let mut console = Vec::new();
    let mut render_requested = false;
    for _ in 0..3 {
        session
            .advance_time(initial.document, Duration::from_secs(2), 1)
            .unwrap();
        match session.wait_for_event(Duration::from_secs(3)).unwrap() {
            RendererEvent::RuntimeUpdate(update) if update.document == initial.document => {
                console.extend(update.runtime.console);
                render_requested |= update.runtime.render_requested;
            }
            RendererEvent::Diagnostic { .. } => {}
            RendererEvent::Presentation(presentation) => {
                panic!(
                    "console-only timer emitted visual revision {}",
                    presentation.revision
                )
            }
            event => panic!("unexpected renderer event after quiet timer: {event:?}"),
        }
        if !console.is_empty() {
            break;
        }
    }
    assert_eq!(console, ["log: timer completed"]);
    assert!(!render_requested);
    session
        .ping(Duration::from_secs(1))
        .expect("runtime update did not wedge the renderer");
    while let Some(event) = session.try_event().unwrap() {
        assert!(
            !matches!(event, RendererEvent::Presentation(_)),
            "quiet timer left a visual presentation queued"
        );
    }
    session.shutdown().expect("shutdown renderer");
}

#[test]
fn script_inserted_stylesheets_and_images_load_after_first_paint() {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut session = RendererSession::launch(options()).expect("launch renderer");
    let initial = load_html_document(
        &session,
        95,
        r#"<!doctype html><head></head><body><div id="card">dynamic card</div><script>
            setTimeout(() => {
                const link = document.createElement('link');
                link.setAttribute('rel', 'stylesheet');
                link.setAttribute('href', '/dynamic.css');
                document.querySelector('head').appendChild(link);
                const image = document.createElement('img');
                image.setAttribute('src', '/dynamic.png');
                image.setAttribute('alt', 'dynamic image');
                document.body.appendChild(image);
            }, 10_000);
        </script></body>"#,
    );

    session
        .advance_time(initial.document, Duration::from_secs(11), 1)
        .expect("run resource-inserting timer");
    let requests = loop {
        match session.wait_for_event(Duration::from_secs(3)).unwrap() {
            RendererEvent::FetchBatch { document, requests } if document == initial.document => {
                break requests;
            }
            RendererEvent::Diagnostic { .. }
            | RendererEvent::RuntimeUpdate(_)
            | RendererEvent::Presentation(_) => {}
            event => panic!("unexpected event while waiting for resources: {event:?}"),
        }
    };
    assert_eq!(requests.len(), 2);
    let placeholder = wait_for_document_presentation(&session, initial.document);
    assert!(
        !placeholder
            .images
            .iter()
            .any(|image| image.url.ends_with("/dynamic.png")),
        "the renderer must present without waiting for the resource batch"
    );
    session
        .ping(Duration::from_secs(1))
        .expect("pending presentation resources blocked renderer heartbeats");
    let sink = session.fetch_response_sink(initial.document);
    let mut style_request = None;
    let mut image_request = None;
    for request in requests {
        match request.head.destination {
            ResourceDestination::Style => style_request = Some(request),
            ResourceDestination::Image => image_request = Some(request),
            destination => panic!("unexpected dynamic destination: {destination:?}"),
        }
    }
    let respond = |request: &RendererFetchRequest, content_type: &str, bytes: Vec<u8>| {
        let request_id = request.head.request_id;
        sink.start(FetchResponseHead {
            request_id,
            result: FetchResponseResult::Success {
                response_type: FetchResponseType::Basic,
                urls: vec![request.head.url.clone()],
                status: 200,
                headers: vec![
                    ("content-type".into(), content_type.into()),
                    ("content-length".into(), bytes.len().to_string()),
                ],
            },
        })
        .unwrap();
        sink.chunk(TransferChunk {
            transfer_id: request_id,
            offset: 0,
            bytes: bytes.clone(),
        })
        .unwrap();
        sink.end(request_id, bytes.len() as u32).unwrap();
    };

    respond(
        &style_request.expect("dynamic stylesheet request"),
        "text/css",
        b"#card { background-color: rgb(1, 2, 3); width: 200px; }".to_vec(),
    );
    let styled = wait_for_document_presentation(&session, initial.document);
    assert!(styled.layout.items.iter().any(|item| {
        matches!(
            item,
            DisplayItem::SolidRect { color, .. } if *color == Color::rgb(1, 2, 3)
        )
    }));
    assert!(
        !styled
            .images
            .iter()
            .any(|image| image.url.ends_with("/dynamic.png")),
        "a completed stylesheet must install without waiting for the image in the same batch"
    );

    respond(
        &image_request.expect("dynamic image request"),
        "image/png",
        test_png(),
    );

    let rendered = loop {
        let rendered = wait_for_document_presentation(&session, initial.document);
        let has_style = rendered.layout.items.iter().any(|item| {
            matches!(
                item,
                DisplayItem::SolidRect { color, .. } if *color == Color::rgb(1, 2, 3)
            )
        });
        let has_image = rendered.layout.items.iter().any(|item| {
            matches!(
                item,
                DisplayItem::Image { url, .. } if url.ends_with("/dynamic.png")
            )
        });
        if has_style && has_image {
            break rendered;
        }
    };
    assert!(rendered.images.iter().any(|image| {
        image.url.ends_with("/dynamic.png") && image.image.width == 2 && image.image.height == 1
    }));
    session.shutdown().expect("shutdown renderer");
}

fn wait_for_document_presentation(
    session: &RendererSession,
    document: better_web_browser::renderer_protocol::DocumentId,
) -> Box<better_web_browser::renderer_protocol::RendererPresentation> {
    loop {
        match session.wait_for_event(Duration::from_secs(3)).unwrap() {
            RendererEvent::Presentation(presentation) if presentation.document == document => {
                return presentation;
            }
            RendererEvent::Diagnostic { .. } | RendererEvent::RuntimeUpdate(_) => {}
            event => panic!("unexpected event while waiting for presentation: {event:?}"),
        }
    }
}

fn test_png() -> Vec<u8> {
    let source = image::RgbaImage::from_pixel(2, 1, image::Rgba([10, 20, 30, 255]));
    let mut bytes = Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(source)
        .write_to(&mut bytes, image::ImageFormat::Png)
        .unwrap();
    bytes.into_inner()
}
