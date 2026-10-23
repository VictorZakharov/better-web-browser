use super::support::*;
use better_web_browser::engine::DisplayItem;
use better_web_browser::renderer_process::{NavigationBody, RendererEvent, RendererSession};
use better_web_browser::renderer_protocol::{
    DocumentId, FetchResponseHead, FetchResponseResult, FetchResponseType, PresentedViewport,
    RendererPresentation, TransferChunk,
};
use std::time::{Duration, Instant};

fn start(content_type: &str) -> (RendererSession, DocumentId, NavigationBody) {
    let session = RendererSession::launch(options()).unwrap();
    let document = DocumentId::new(750).unwrap();
    let mut start = document_start(document, 0);
    start.content_type = content_type.into();
    let body = NavigationBody::default();
    session
        .load_streaming_document(start, empty_document_state(), body.clone())
        .unwrap();
    (session, document, body)
}

fn until_text(
    session: &RendererSession,
    document: DocumentId,
    needle: &str,
) -> RendererPresentation {
    presentations_until_text(session, document, needle).1
}

/// Waits for a presentation containing `needle`, acknowledging and counting the
/// presentations up to it as the browser would.
fn presentations_until_text(
    session: &RendererSession,
    document: DocumentId,
    needle: &str,
) -> (usize, RendererPresentation) {
    let mut presentations = 0;
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        assert!(Instant::now() < deadline, "never painted {needle}");
        let next = match session.wait_for_event(Duration::from_secs(3)).unwrap() {
            RendererEvent::Presentation(presentation) => {
                assert!(
                    presentation.runtime.errors.is_empty(),
                    "{:?}",
                    presentation.runtime.errors
                );
                presentations += 1;
                acknowledge(session, &presentation);
                if text(&presentation).contains(needle) {
                    return (presentations, *presentation);
                }
                presentation.next_timer_micros
            }
            RendererEvent::RuntimeUpdate(update) => {
                assert!(
                    update.runtime.errors.is_empty(),
                    "{:?}",
                    update.runtime.errors
                );
                update.next_timer_micros
            }
            RendererEvent::Diagnostic { .. } | RendererEvent::StorageMutation(_) => continue,
            event => panic!("unexpected streaming navigation event: {event:?}"),
        };
        if next.is_some() {
            session
                .advance_time(document, Duration::from_millis(10), 8)
                .unwrap();
        }
    }
}

fn text(presentation: &RendererPresentation) -> String {
    presentation
        .layout
        .items
        .iter()
        .filter_map(|item| match item {
            DisplayItem::Text { text, .. } => Some(&**text),
            _ => None,
        })
        .collect()
}

#[test]
fn streamed_prefix_scripts_and_timers_run_before_network_eof_without_completing_load() {
    let _serial = SERIAL.lock().unwrap_or_else(|error| error.into_inner());
    let (mut session, document, body) = start("text/html; charset=utf-8");
    body.append(br#"<!doctype html><p id=status>prefix</p><script>
      window.trace=[]; window.mark=x=>{trace.push(x);document.querySelector('#status').textContent=trace.join('|')};
      mark('prefix-'+document.readyState+'-'+(document.querySelector('#tail')===null));
      document.addEventListener('DOMContentLoaded',()=>mark('DCL'));
      window.addEventListener('load',()=>mark('load'));
      setTimeout(()=>mark('timer-'+document.readyState),0);
      </script>"#).unwrap();
    until_text(&session, document, "prefix-loading-true");
    session
        .advance_time(document, Duration::from_millis(10), 8)
        .unwrap();
    let prefix = until_text(&session, document, "timer-loading");
    assert!(!text(&prefix).contains("DCL"));
    session.ping(Duration::from_secs(1)).unwrap();
    body.append(b"<p id=tail>tail</p><script>mark('tail-'+document.readyState)</script>")
        .unwrap();
    let tail = until_text(&session, document, "tail-loading");
    assert!(!text(&tail).contains("DCL"));
    body.finish(Ok(()));
    session
        .advance_time(document, Duration::from_millis(10), 8)
        .unwrap();
    until_text(&session, document, "tail-loading|DCL|load");
    session.shutdown().unwrap();
}

#[test]
fn later_real_meta_restarts_from_memory_and_preserves_monotonic_presentations() {
    let _serial = SERIAL.lock().unwrap_or_else(|error| error.into_inner());
    let (mut session, document, body) = start("text/html");
    body.append(b"<!doctype html><!-- <meta charset=windows-1252> --><p>prefix \xe9</p>")
        .unwrap();
    let prefix = until_text(&session, document, "prefix");
    body.append(b"<meta charset=windows-1252><p id=status>pending</p><script>document.querySelector('#status').textContent=document.characterSet</script>").unwrap();
    let replay = until_text(&session, document, "windows-1252");
    assert!(text(&replay).contains("prefix é"), "{}", text(&replay));
    assert!(replay.revision > prefix.revision);
    body.append(b"<p>tail \x80</p>").unwrap();
    body.finish(Ok(()));
    until_text(&session, document, "tail €");
    session.shutdown().unwrap();
}

#[test]
fn cancellation_discards_old_stream_before_a_replacement_document() {
    let _serial = SERIAL.lock().unwrap_or_else(|error| error.into_inner());
    let (mut session, document, body) = start("text/html; charset=utf-8");
    body.append(b"<!doctype html><p>old prefix</p>").unwrap();
    until_text(&session, document, "old prefix");
    session.cancel_document(document).unwrap();
    body.append(b"<script>throw Error('stale script')</script>")
        .unwrap();
    body.finish(Ok(()));
    let next = load_html_document(&session, 751, "<!doctype html><p>replacement document</p>");
    assert!(text(&next).contains("replacement document"));
    assert!(next.runtime.errors.is_empty());
    session.shutdown().unwrap();
}

#[test]
fn failed_response_is_contained_and_a_new_document_still_loads() {
    let _serial = SERIAL.lock().unwrap_or_else(|error| error.into_inner());
    let (mut session, document, body) = start("text/html; charset=utf-8");
    body.append(b"<!doctype html><p>partial document</p>")
        .unwrap();
    until_text(&session, document, "partial document");
    body.finish(Err("connection interrupted".into()));
    loop {
        match session.wait_for_event(Duration::from_secs(5)).unwrap() {
            RendererEvent::DocumentFailed {
                document: failed,
                detail,
            } => {
                assert_eq!(failed, document);
                assert!(detail.contains("connection interrupted"), "{detail}");
                break;
            }
            RendererEvent::Presentation(_)
            | RendererEvent::RuntimeUpdate(_)
            | RendererEvent::Diagnostic { .. } => {}
            event => panic!("unexpected failure event: {event:?}"),
        }
    }
    session.ping(Duration::from_secs(1)).unwrap();
    let next = load_html_document(&session, 752, "<!doctype html><p>recovered document</p>");
    assert!(text(&next).contains("recovered document"));
    session.shutdown().unwrap();
}

#[test]
fn encoding_restart_preserves_pending_storage_journal_and_write_sequence() {
    let _serial = SERIAL.lock().unwrap_or_else(|error| error.into_inner());
    let (mut session, document, body) = start("text/html");
    body.append(
        br#"<!doctype html><p id=status>pending</p><script>
      localStorage.setItem('count', String(Number(localStorage.getItem('count') || 0)+1));
      document.querySelector('#status').textContent='count='+localStorage.getItem('count');
      </script>"#,
    )
    .unwrap();
    until_text(&session, document, "count=1");
    body.append(b"<meta charset=windows-1252><p>\xe9</p>")
        .unwrap();
    let mut second_write = false;
    loop {
        match session.wait_for_event(Duration::from_secs(5)).unwrap() {
            RendererEvent::StorageMutation(write) => {
                assert_eq!(write.sequence, 2);
                second_write = true;
            }
            RendererEvent::Presentation(p) => {
                assert!(p.runtime.errors.is_empty(), "{:?}", p.runtime.errors);
                if text(&p).contains("count=2") {
                    assert!(second_write);
                    break;
                }
            }
            RendererEvent::RuntimeUpdate(u) => {
                assert!(u.runtime.errors.is_empty());
            }
            RendererEvent::Diagnostic { .. } => {}
            event => panic!("unexpected restart event: {event:?}"),
        }
    }
    body.finish(Ok(()));
    session.shutdown().unwrap();
}

#[test]
fn stylesheet_bytes_go_ahead_of_document_chunks_that_are_already_waiting() {
    let _serial = SERIAL.lock().unwrap_or_else(|error| error.into_inner());
    let (mut session, document, body) = start("text/html; charset=utf-8");
    body.append(b"<!doctype html><head><link rel=stylesheet href=/sheet.css></head><body>")
        .unwrap();
    let request = loop {
        match session.wait_for_event(Duration::from_secs(3)).unwrap() {
            RendererEvent::FetchBatch { requests, .. } => {
                if let Some(request) = requests
                    .into_iter()
                    .find(|request| request.head.url.ends_with("/sheet.css"))
                {
                    break request;
                }
            }
            RendererEvent::RuntimeUpdate(_)
            | RendererEvent::Diagnostic { .. }
            | RendererEvent::StorageMutation(_) => {}
            event => panic!("unexpected event before the stylesheet request: {event:?}"),
        }
    };
    // About eight document chunks are waiting when the stylesheet's sixteen chunks arrive.
    // Taking turns would deliver the whole document before the stylesheet ends.
    let paragraph = format!("<p>{}</p>", "filler ".repeat(140));
    for slice in paragraph.repeat(512).as_bytes().chunks(64 * 1024) {
        body.append(slice).unwrap();
    }
    body.append(b"<p>end of document</p>").unwrap();
    body.finish(Ok(()));
    let mut sheet = format!("/*{}*/", " ".repeat(15 * 64 * 1024)).into_bytes();
    sheet.extend_from_slice(b"p{color:rgb(1,2,3)}");
    let id = request.head.request_id;
    let sink = session.fetch_response_sink(document);
    sink.start(FetchResponseHead {
        request_id: id,
        result: FetchResponseResult::Success {
            response_type: FetchResponseType::Basic,
            urls: vec![request.head.url.clone()],
            status: 200,
            headers: vec![("content-type".into(), "text/css".into())],
        },
    })
    .unwrap();
    for (index, bytes) in sheet.chunks(64 * 1024).enumerate() {
        sink.chunk(TransferChunk {
            transfer_id: id,
            offset: (index * 64 * 1024) as u32,
            bytes: bytes.to_vec(),
        })
        .unwrap();
    }
    sink.end(id, sheet.len() as u32).unwrap();
    let first = until_text(&session, document, "filler");
    assert!(first.layout.items.iter().any(|item| matches!(
        item,
        DisplayItem::Text { color, .. } if (color.red, color.green, color.blue) == (1, 2, 3)
    )));
    assert!(
        !text(&first).contains("end of document"),
        "the first frame waited for the whole document"
    );
    session.shutdown().unwrap();
}

/// Appends `count` paragraphs, each padded by a comment to fill a document chunk of its own.
fn parts(body: &NavigationBody, count: usize) {
    for index in 0..count {
        let filler = "x".repeat(60 * 1024);
        body.append(format!("<p class=part>part {index}</p><!--{filler}-->").as_bytes())
            .unwrap();
    }
}

#[test]
fn content_parsed_after_the_first_frame_waits_for_the_parser() {
    let _serial = SERIAL.lock().unwrap_or_else(|error| error.into_inner());
    let (mut session, document, body) = start("text/html; charset=utf-8");
    body.append(b"<!doctype html><p>first part</p>").unwrap();
    until_text(&session, document, "first part");
    // Twelve chunks arrive back to back. Each would otherwise restyle, lay out and present the
    // growing document; the next frame waits until the input ends instead.
    parts(&body, 12);
    body.append(b"<p>end</p>").unwrap();
    body.finish(Ok(()));
    let (presentations, last) = presentations_until_text(&session, document, "end");
    assert!(presentations <= 3, "{presentations} frames while parsing");
    assert!(text(&last).contains("part 0"));
    session.shutdown().unwrap();
}

#[test]
fn a_resize_while_a_parsing_frame_waits_styles_the_parsed_content() {
    let _serial = SERIAL.lock().unwrap_or_else(|error| error.into_inner());
    let (mut session, document, body) = start("text/html; charset=utf-8");
    body.append(b"<!doctype html><style>.part{color:rgb(7,8,9)}</style><p>first part</p>")
        .unwrap();
    until_text(&session, document, "first part");
    // The resize presents while the parsed paragraphs still wait for a frame; it must restyle
    // them although no parser checkpoint refreshed their styles.
    parts(&body, 4);
    session
        .update_viewport(
            document,
            PresentedViewport {
                width: 640.0,
                style_width: 640.0,
                ..document_start(document, 0).viewport
            },
        )
        .unwrap();
    let resized = until_text(&session, document, "part 0");
    body.append(b"<p class=part>part end</p>").unwrap();
    body.finish(Ok(()));
    let last = until_text(&session, document, "part end");
    for presentation in [resized, last] {
        assert!(presentation.layout.items.iter().all(|item| match item {
            DisplayItem::Text { text, color, .. } if text.starts_with("part") =>
                (color.red, color.green, color.blue) == (7, 8, 9),
            _ => true,
        }));
    }
    session.shutdown().unwrap();
}
