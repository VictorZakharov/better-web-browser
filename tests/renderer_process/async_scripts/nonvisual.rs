use super::*;

#[test]
fn removing_hidden_slot_rebuilds_for_content_redistributed_to_a_visible_slot() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let mut driver = Driver::new(
        r#"<!doctype html><head><script async src=/move.js></script></head>
        <body><div id=host><p>slotted visible</p></div><p>ready</p><script>
        const shadow = document.querySelector('#host').attachShadow({mode:'open'});
        shadow.innerHTML = '<section style="display:none"><slot id="hidden"></slot></section><slot></slot>';
        window.hiddenSlot = shadow.querySelector('#hidden');
        </script>"#,
    );
    let before = driver.until_text("ready");
    assert!(!painted_text(&before).contains("slotted visible"));
    driver.respond("move.js", "hiddenSlot.remove()", 200);
    driver.until_text("slotted visible");
    driver.session.shutdown().unwrap();
}

#[test]
fn removing_completed_head_script_keeps_geometry_and_updates_metadata() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let mut driver = Driver::new(
        "<!doctype html><head><title>before</title><script id=quiet async src=/quiet.js></script></head><body><p>unchanged article</p>",
    );
    driver.until_text("unchanged article");
    driver.advance();
    driver.until_idle();
    driver.respond(
        "quiet.js",
        "document.querySelector('#quiet').remove(); document.querySelector('title').firstChild.data='after';",
        200,
    );
    let after = driver.until_text("unchanged article");
    assert_eq!(after.title, "after");
    assert_eq!(
        after.load.text_measure_count, 0,
        "hidden removal rebuilt article geometry"
    );
    driver.session.shutdown().unwrap();
}

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
