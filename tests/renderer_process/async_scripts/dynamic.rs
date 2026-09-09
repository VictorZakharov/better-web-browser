use super::*;

const DYNAMIC: &str =
    include_str!("../../../benchmarks/alpha/fixtures/dynamic-script-readiness.html");
const ORDERED: &str = include_str!("../../../benchmarks/alpha/fixtures/dynamic-ordered.js");

#[test]
fn dynamic_async_and_ordered_elements_wake_on_completion_not_download_polling() {
    let _serial = SERIAL.lock().unwrap_or_else(|error| error.into_inner());
    let mut driver = Driver::new(DYNAMIC);
    driver.until_text("Fast content pending");
    driver.advance();
    driver.until_idle();
    assert_eq!(
        driver.requests.len(),
        5,
        "identical in-flight URLs retain independent element owners"
    );
    driver.respond("dynamic-ordered.js?delay_ms=100", ORDERED, 200);
    // The second ordered response is deliberately ready before the first. A ping proves the
    // renderer is not waiting synchronously for the missing earlier source.
    driver.session.ping(Duration::from_secs(1)).unwrap();
    driver.respond("async-fast.js", FAST, 200);
    let fast = driver.until_text("Fast content ready (2 elements)");
    let text = painted_text(&fast);
    assert!(text.contains("Slow content pending"));
    assert!(text.contains("Ordered content pending"));
    driver.respond("async-missing.js", "missing", 404);
    driver.until_text("error:missing-b:true");
    driver.respond("dynamic-ordered.js?delay_ms=2000", ORDERED, 200);
    let ordered = driver.until_text("Ordered content ready (2 elements)");
    let trace = painted_text(&ordered).replace(' ', "");
    assert!(trace.find("run:ordered-first").unwrap() < trace.find("run:ordered-second").unwrap());
    driver.respond("async-slow.js", SLOW, 200);
    let final_page = driver.until_text("Slow content ready");
    assert_eq!(final_page.title, "Dynamic scripts complete");
    driver.session.shutdown().unwrap();
}

#[test]
fn a_new_dynamic_request_starts_while_an_older_batch_remains_pending() {
    let _serial = SERIAL.lock().unwrap_or_else(|error| error.into_inner());
    let mut driver = Driver::new(
        r#"<!doctype html><head></head><body><div id=status>pending</div><script>
      for (const src of ['/slow.js', '/fast.js']) {
        const s = document.createElement('script'); s.src = src; document.head.appendChild(s);
      }
    </script>"#,
    );
    driver.until_text("pending");
    driver.respond(
        "fast.js",
        r#"
      const nested = document.createElement('script'); nested.src = '/nested.js';
      nested.onload = () => document.getElementById('status').textContent = 'nested ready';
      document.head.appendChild(nested);
    "#,
        200,
    );
    let deadline = Instant::now() + Duration::from_secs(3);
    while !driver.requests.keys().any(|url| url.contains("nested.js")) {
        assert!(Instant::now() < deadline);
        match driver
            .session
            .wait_for_event(Duration::from_secs(1))
            .unwrap()
        {
            RendererEvent::FetchBatch { requests, .. } => {
                for request in requests {
                    driver.requests.insert(request.head.url.clone(), request);
                }
            }
            RendererEvent::RuntimeUpdate(update) if update.next_timer_micros.is_some() => {
                driver.advance()
            }
            RendererEvent::Presentation(page) if page.next_timer_micros.is_some() => {
                driver.advance()
            }
            RendererEvent::RuntimeUpdate(_)
            | RendererEvent::Presentation(_)
            | RendererEvent::Diagnostic { .. } => {}
            event => panic!("unexpected event: {event:?}"),
        }
    }
    driver.respond("nested.js", "window.nested = true;", 200);
    driver.until_text("nested ready");
    driver.session.ping(Duration::from_secs(1)).unwrap();
    driver.session.cancel_document(driver.document).unwrap();
    driver.session.shutdown().unwrap();
}
