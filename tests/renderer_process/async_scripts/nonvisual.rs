use super::*;

#[test]
fn downloaded_script_source_does_not_invalidate_layout_but_script_mutations_do() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let mut driver = Driver::new(
        r#"<!doctype html><p id=status>unchanged</p>
        <script async src=/quiet.js onload="console.log('quiet load')"></script>
        <script async src=/change.js></script>"#,
    );
    driver.until_text("unchanged");
    driver.advance();
    driver.until_idle();
    driver.respond("quiet.js", "console.log('quiet execution')", 200);
    let mut messages = Vec::new();
    for _ in 0..30 {
        match driver
            .session
            .wait_for_event(Duration::from_secs(3))
            .unwrap()
        {
            RendererEvent::RuntimeUpdate(update) => {
                assert!(
                    update.runtime.errors.is_empty(),
                    "{:?}",
                    update.runtime.errors
                );
                messages.extend(update.runtime.console);
                if messages.iter().any(|s| s.contains("quiet load"))
                    && update.next_timer_micros.is_none()
                {
                    break;
                }
                if update.next_timer_micros.is_some() {
                    driver.advance();
                }
            }
            RendererEvent::Diagnostic { .. } => {}
            RendererEvent::Presentation(_) => {
                panic!("nonvisual source/execution/load repainted the document")
            }
            event => panic!("unexpected {event:?}"),
        }
    }
    assert!(messages.iter().any(|s| s.contains("quiet execution")));
    assert!(messages.iter().any(|s| s.contains("quiet load")));
    driver.respond(
        "change.js",
        "document.getElementById('status').textContent='updated';",
        200,
    );
    driver.until_text("updated");
    driver.session.shutdown().unwrap();
}

#[test]
fn nonvisual_resource_error_still_starts_followup_fetch_and_paints_its_mutation() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let mut driver = Driver::new(
        r#"<!doctype html><p id=status>unchanged</p>
        <script async src=/missing.js onerror="const s=document.createElement('script');
        s.src='/recovery.js'; document.head.appendChild(s);"></script>"#,
    );
    driver.until_text("unchanged");
    driver.advance();
    driver.until_idle();
    driver.respond("missing.js", "missing", 404);
    for _ in 0..30 {
        match driver
            .session
            .wait_for_event(Duration::from_secs(3))
            .unwrap()
        {
            RendererEvent::FetchBatch { requests, .. } => {
                for request in requests {
                    driver.requests.insert(request.head.url.clone(), request);
                }
                if driver
                    .requests
                    .keys()
                    .any(|url| url.ends_with("/recovery.js"))
                {
                    break;
                }
            }
            RendererEvent::RuntimeUpdate(update) => {
                assert!(
                    update.runtime.errors.is_empty(),
                    "{:?}",
                    update.runtime.errors
                );
                if update.next_timer_micros.is_some() {
                    driver.advance();
                }
            }
            RendererEvent::Diagnostic { .. } => {}
            RendererEvent::Presentation(presentation) => {
                assert!(
                    presentation.runtime.errors.is_empty(),
                    "{:?}",
                    presentation.runtime.errors
                );
                if presentation.next_timer_micros.is_some() {
                    driver.advance();
                }
            }
            event => panic!("unexpected {event:?}"),
        }
    }
    driver.respond(
        "recovery.js",
        "document.getElementById('status').textContent='recovered';",
        200,
    );
    driver.until_text("recovered");
    driver.session.shutdown().unwrap();
}
