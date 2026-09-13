use super::*;

fn blocked_until_request(driver: &mut Driver, suffix: &str) {
    for _ in 0..30 {
        if driver.requests.keys().any(|url| url.ends_with(suffix)) {
            return;
        }
        match driver
            .session
            .wait_for_event(Duration::from_secs(3))
            .unwrap()
        {
            RendererEvent::FetchBatch { requests, .. } => {
                for request in requests {
                    driver.requests.insert(request.head.url.clone(), request);
                }
            }
            RendererEvent::RuntimeUpdate(update) => {
                assert!(
                    update.runtime.errors.is_empty(),
                    "{:?}",
                    update.runtime.errors
                );
                if update.next_timer_micros == Some(0) {
                    driver.advance();
                }
            }
            RendererEvent::Diagnostic { .. } => {}
            RendererEvent::Presentation(_) => {
                panic!("painted before the blocking stylesheet settled")
            }
            event => panic!("unexpected event: {event:?}"),
        }
    }
    panic!("never requested {suffix}");
}

#[test]
fn initial_stylesheets_block_paint_not_async_scripts_or_heartbeats() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let mut driver = Driver::new(
        "<!doctype html><head><link rel=stylesheet href=/slow.css><script async src=/fast.js></script></head><body><p id=status>styled first</p>",
    );
    blocked_until_request(&mut driver, "slow.css");
    blocked_until_request(&mut driver, "fast.js");
    driver.respond("fast.js", "console.log('async while paint blocked')", 200);
    let mut ran = false;
    for _ in 0..30 {
        match driver
            .session
            .wait_for_event(Duration::from_secs(3))
            .unwrap()
        {
            RendererEvent::RuntimeUpdate(update) => {
                ran |= update
                    .runtime
                    .console
                    .iter()
                    .any(|line| line.contains("async while paint blocked"));
                if ran {
                    break;
                }
                if update.next_timer_micros.is_some() {
                    driver.advance();
                }
            }
            RendererEvent::FetchBatch { requests, .. } => {
                for request in requests {
                    driver.requests.insert(request.head.url.clone(), request);
                }
            }
            RendererEvent::Diagnostic { .. } => {}
            RendererEvent::Presentation(_) => panic!("unstyled presentation escaped"),
            event => panic!("{event:?}"),
        }
    }
    assert!(ran);
    driver.session.ping(Duration::from_secs(1)).unwrap();
    driver.respond_bytes(
        "slow.css",
        b"#status{color:rgb(4,5,6);width:300px}",
        "text/css",
        200,
    );
    let first = driver.until_text("styled first");
    assert!(first.layout.items.iter().any(|item| matches!(item,DisplayItem::Text{color,..} if color.red==4 && color.green==5 && color.blue==6)));
    driver.session.shutdown().unwrap();
}

#[test]
fn stylesheet_failure_releases_the_first_presentation_without_clock_polling() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let mut driver = Driver::new(
        "<!doctype html><head><link rel=stylesheet href=/missing.css></head><body><p>visible after failure</p>",
    );
    blocked_until_request(&mut driver, "missing.css");
    driver.session.ping(Duration::from_secs(1)).unwrap();
    driver.respond_bytes("missing.css", b"missing", "text/css", 404);
    driver.until_text("visible after failure");
    driver.session.shutdown().unwrap();
}

#[test]
fn removing_a_pending_head_stylesheet_releases_rendering() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let mut driver = Driver::new(
        "<!doctype html><head><link id=sheet rel=stylesheet href=/slow.css><script async src=/remove.js></script></head><body><p>released by removal</p>",
    );
    blocked_until_request(&mut driver, "slow.css");
    blocked_until_request(&mut driver, "remove.js");
    driver.respond(
        "remove.js",
        "document.querySelector('#sheet').remove()",
        200,
    );
    driver.until_text("released by removal");
    driver.session.shutdown().unwrap();
}

#[test]
fn disabled_alternate_nonmatching_and_body_stylesheets_do_not_block_first_paint() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    for link in [
        "<head><link disabled rel=stylesheet href=/slow.css></head><body>",
        "<head><link rel='alternate stylesheet' title=other href=/slow.css></head><body>",
        "<head><link media=print rel=stylesheet href=/slow.css></head><body>",
        "<body><link rel=stylesheet href=/slow.css>",
    ] {
        let mut driver = Driver::new(&format!("<!doctype html>{link}<p>not blocked</p>"));
        driver.until_text("not blocked");
        driver.session.shutdown().unwrap();
    }
}

#[test]
fn explicit_head_blocker_created_before_body_remains_blocking_after_body_insertion() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let mut driver = Driver::new(
        r#"<!doctype html><head><script>
      const sheet = document.createElement('link');
      sheet.setAttribute('blocking', 'render'); sheet.rel='stylesheet'; sheet.href='/explicit.css';
      document.head.appendChild(sheet);
    </script></head><body><p>explicit styled content</p>"#,
    );
    blocked_until_request(&mut driver, "explicit.css");
    driver.session.ping(Duration::from_secs(1)).unwrap();
    driver.respond_bytes("explicit.css", b"p{color:rgb(4,5,6)}", "text/css", 200);
    let first = driver.until_text("explicit styled content");
    assert!(first.layout.items.iter().any(|item| matches!(item, DisplayItem::Text{color,..} if color.red==4 && color.green==5 && color.blue==6)));
    driver.session.shutdown().unwrap();
}
