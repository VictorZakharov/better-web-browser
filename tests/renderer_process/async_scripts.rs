use super::support::*;
use better_web_browser::engine::DisplayItem;
use better_web_browser::renderer_process::{RendererEvent, RendererSession};
use better_web_browser::renderer_protocol::{
    DocumentId, FetchResponseHead, FetchResponseResult, FetchResponseType, RendererFetchRequest,
    RendererPresentation, TransferChunk,
};
use std::collections::HashMap;
use std::time::{Duration, Instant};

const HTML: &str = include_str!("../../benchmarks/alpha/fixtures/async-script-readiness.html");
const FAST: &str = include_str!("../../benchmarks/alpha/fixtures/async-fast.js");
const SLOW: &str = include_str!("../../benchmarks/alpha/fixtures/async-slow.js");
#[path = "async_scripts/dynamic.rs"]
mod dynamic;

#[test]
fn async_scripts_execute_ready_elements_and_fail_each_owner_without_waiting_for_slow_fetch() {
    let _serial = SERIAL.lock().unwrap_or_else(|error| error.into_inner());
    let mut driver = Driver::new(HTML);
    driver.until_text("Fast content pending");
    // The first layout may queue one geometry-observer checkpoint; after that,
    // only the network completion should wake this otherwise idle document.
    driver.advance();
    driver.until_idle();
    assert_eq!(
        driver.requests.len(),
        3,
        "shared URLs use shared downloads, not shared execution"
    );

    driver.respond("async-fast.js", FAST, 200);
    let fast = driver.until_text("Fast content ready (2 elements)");
    let text = painted_text(&fast);
    assert!(text.contains("Slow content pending"));
    let compact = text.replace(' ', "");
    assert!(
        compact.contains("run:fast-a|micro:fast-a:false|load:fast-a:true:true"),
        "{text}"
    );
    assert!(
        compact.contains("run:fast-b|micro:fast-b:false|load:fast-b:true:true"),
        "{text}"
    );
    driver.session.ping(Duration::from_secs(1)).unwrap();

    driver.respond("async-missing.js", "missing", 404);
    let failed = driver.until_text("error:missing-b:true");
    assert!(painted_text(&failed).contains("error:missing-a:true"));
    driver.respond("async-slow.js", SLOW, 200);
    driver.until_text("Slow content ready");
    driver.session.shutdown().unwrap();
}

#[test]
fn prepared_async_element_keeps_original_source_after_detachment_and_cannot_execute_in_another_document()
 {
    let _serial = SERIAL.lock().unwrap_or_else(|error| error.into_inner());
    let html = r#"<!doctype html><title>preparation</title><div id=status>pending</div>
        <script id=detached async src=/original.js></script>
        <script id=adopted async src=/foreign.js></script>
        <script>
          const detached = document.getElementById('detached');
          detached.remove(); detached.src = '/replacement.js'; document.head.appendChild(detached);
          const adopted = document.getElementById('adopted');
          document.implementation.createHTMLDocument('').adoptNode(adopted);
        </script>"#;
    let mut driver = Driver::new(html);
    driver.until_text("pending");
    assert_eq!(driver.requests.len(), 2);
    driver.respond(
        "foreign.js",
        "throw new Error('wrong document executed');",
        200,
    );
    driver.respond(
        "original.js",
        "document.querySelector('#status').textContent = 'original ready';",
        200,
    );
    driver.until_text("original ready");
    assert!(
        !driver
            .requests
            .keys()
            .any(|url| url.contains("replacement.js"))
    );
    driver.session.shutdown().unwrap();
}

struct Driver {
    session: RendererSession,
    document: DocumentId,
    requests: HashMap<String, RendererFetchRequest>,
}

#[test]
fn resources_discovered_by_a_ready_script_do_not_wait_for_the_slow_script_batch() {
    let _serial = SERIAL.lock().unwrap_or_else(|error| error.into_inner());
    let mut driver = Driver::new(
        r#"<!doctype html><div id=status>pending</div>
        <script async src=/slow.js></script><script async src=/fast.js></script>"#,
    );
    driver.until_text("pending");
    driver.respond("fast.js", r#"
        const link = document.createElement('link'); link.rel = 'stylesheet'; link.href = '/late.css';
        link.onload = () => document.getElementById('status').textContent = 'late stylesheet ready';
        document.head.appendChild(link);
    "#, 200);
    loop {
        match driver
            .session
            .wait_for_event(Duration::from_secs(3))
            .unwrap()
        {
            RendererEvent::FetchBatch { requests, .. } => {
                for request in requests {
                    driver.requests.insert(request.head.url.clone(), request);
                }
                if driver.requests.keys().any(|url| url.contains("late.css")) {
                    break;
                }
            }
            RendererEvent::RuntimeUpdate(update) => {
                if update.next_timer_micros.is_some() {
                    driver.advance();
                }
            }
            RendererEvent::Presentation(presentation) => {
                if presentation.next_timer_micros.is_some() {
                    driver.advance();
                }
            }
            RendererEvent::Diagnostic { .. } => {}
            event => panic!("unexpected event: {event:?}"),
        }
    }
    driver.respond("late.css", "#status { color: green; }", 200);
    driver.until_text("late stylesheet ready");
    driver.session.shutdown().unwrap();
}

impl Driver {
    fn new(html: &str) -> Self {
        let session = RendererSession::launch(options()).unwrap();
        let document = DocumentId::new(250).unwrap();
        session
            .load_document(
                document_start(document, html.len()),
                empty_document_state(),
                html.as_bytes().to_vec(),
            )
            .unwrap();
        Self {
            session,
            document,
            requests: HashMap::new(),
        }
    }

    fn until_text(&mut self, expected: &str) -> RendererPresentation {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            assert!(Instant::now() < deadline, "never painted {expected}");
            match self.session.wait_for_event(Duration::from_secs(3)).unwrap() {
                RendererEvent::FetchBatch { requests, .. } => {
                    for request in requests {
                        self.requests.insert(request.head.url.clone(), request);
                    }
                }
                RendererEvent::Presentation(presentation) => {
                    assert!(
                        presentation.runtime.errors.is_empty(),
                        "{:?}",
                        presentation.runtime.errors
                    );
                    if painted_text(&presentation).contains(expected) {
                        return *presentation;
                    }
                    if presentation.next_timer_micros.is_some() {
                        self.advance();
                    }
                }
                RendererEvent::RuntimeUpdate(update) => {
                    assert!(
                        update.runtime.errors.is_empty(),
                        "{:?}",
                        update.runtime.errors
                    );
                    if update.next_timer_micros.is_some() {
                        self.advance();
                    }
                }
                RendererEvent::Diagnostic { .. } => {}
                event => panic!("unexpected event: {event:?}"),
            }
        }
    }

    fn advance(&self) {
        self.session
            .advance_time(self.document, Duration::from_millis(1), 1)
            .unwrap();
    }

    fn until_idle(&mut self) {
        for _ in 0..8 {
            let next = match self.session.wait_for_event(Duration::from_secs(3)).unwrap() {
                RendererEvent::RuntimeUpdate(update) => update.next_timer_micros,
                RendererEvent::Presentation(presentation) => presentation.next_timer_micros,
                RendererEvent::Diagnostic { .. } => continue,
                RendererEvent::FetchBatch { requests, .. } => {
                    for request in requests {
                        self.requests.insert(request.head.url.clone(), request);
                    }
                    continue;
                }
                event => panic!("unexpected event while waiting for idle: {event:?}"),
            };
            if next.is_none() {
                return;
            }
            self.advance();
        }
        panic!("pending async downloads must not cause clock polling");
    }

    fn respond(&self, suffix: &str, source: &str, status: u16) {
        let request = self
            .requests
            .iter()
            .find(|(url, _)| url.contains(suffix))
            .unwrap()
            .1;
        let id = request.head.request_id;
        let sink = self.session.fetch_response_sink(self.document);
        sink.start(FetchResponseHead {
            request_id: id,
            result: FetchResponseResult::Success {
                response_type: FetchResponseType::Basic,
                urls: vec![request.head.url.clone()],
                status,
                headers: vec![("content-type".into(), "text/javascript".into())],
            },
        })
        .unwrap();
        sink.chunk(TransferChunk {
            transfer_id: id,
            offset: 0,
            bytes: source.as_bytes().to_vec(),
        })
        .unwrap();
        sink.end(id, source.len() as u32).unwrap();
    }
}

fn painted_text(presentation: &RendererPresentation) -> String {
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
