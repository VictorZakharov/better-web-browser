use super::support::*;
use better_web_browser::engine::DisplayItem;
use better_web_browser::renderer_process::{NavigationBody, RendererEvent, RendererSession};
use better_web_browser::renderer_protocol::{DocumentId, RendererPresentation};
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
                if text(&presentation).contains(needle) {
                    return *presentation;
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
            DisplayItem::Text { text, .. } => Some(text.as_str()),
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
