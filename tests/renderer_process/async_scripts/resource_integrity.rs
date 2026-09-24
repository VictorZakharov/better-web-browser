use super::rendering::blocked_until_request;
use super::*;
use base64::Engine as _;
use sha2::{Digest, Sha256, Sha512};

#[test]
fn a_link_preload_does_not_apply_unused_css() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let mut driver = Driver::new(
        "<!doctype html><head><link id=hint rel=preload as=style href=/unused.css></head>\
         <body><p id=status>pending</p><script>\
         document.getElementById('hint').onload=()=>document.getElementById('status').textContent='preload loaded';\
         </script>",
    );
    driver.until_text("pending");
    assert!(
        driver
            .requests
            .keys()
            .any(|url| url.ends_with("unused.css"))
    );
    driver.respond_bytes(
        "unused.css",
        b"#status { color: rgb(255,0,0) }",
        "text/css",
        200,
    );
    let presentation = driver.until_text("preload loaded");
    assert!(!presentation.layout.items.iter().any(|item| matches!(item,
        DisplayItem::Text { color, .. } if color.red == 255 && color.green == 0 && color.blue == 0
    )));
    driver.session.shutdown().unwrap();
}

#[test]
fn matching_stylesheet_consumes_preload_without_a_second_request() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let mut driver = Driver::new(
        "<!doctype html><head><link rel=preload as=style href=/shared.css>\
         <link rel=stylesheet href=/shared.css></head><body><p id=status>pending</p>\
         <script>document.getElementById('status').textContent='stylesheet used';</script>",
    );
    blocked_until_request(&mut driver, "shared.css");
    assert!(
        driver
            .requests
            .keys()
            .any(|url| url.ends_with("shared.css"))
    );
    let request_id = driver
        .requests
        .values()
        .find(|request| request.head.url.ends_with("shared.css"))
        .unwrap()
        .head
        .request_id;
    driver.respond_bytes(
        "shared.css",
        b"#status { color: rgb(255,0,0) }",
        "text/css",
        200,
    );
    let presentation = driver.until_text("stylesheet used");
    assert_eq!(
        driver
            .requests
            .values()
            .find(|request| request.head.url.ends_with("shared.css"))
            .unwrap()
            .head
            .request_id,
        request_id,
        "stylesheet made a duplicate network request"
    );
    assert!(presentation.layout.items.iter().any(|item| matches!(item,
        DisplayItem::Text { color, .. } if color.red == 255 && color.green == 0 && color.blue == 0
    )));
    driver.session.shutdown().unwrap();
}

#[test]
fn completed_preload_is_reused_by_a_dynamically_inserted_stylesheet() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let mut driver = Driver::new(
        "<!doctype html><head><link id=hint rel=preload as=style href=/shared.css></head>\
         <body><p id=status>pending</p><script>\
         document.getElementById('hint').onload=()=>{\
           const sheet=document.createElement('link');sheet.rel='stylesheet';sheet.href='/shared.css';\
           sheet.onload=()=>document.getElementById('status').textContent='stylesheet used';\
           document.getElementById('status').textContent='preload loaded';\
           document.head.appendChild(sheet);\
         };\
         </script>",
    );
    driver.until_text("pending");
    assert!(
        driver
            .requests
            .keys()
            .any(|url| url.ends_with("shared.css"))
    );
    let request_id = driver
        .requests
        .values()
        .find(|request| request.head.url.ends_with("shared.css"))
        .unwrap()
        .head
        .request_id;
    driver.respond_bytes(
        "shared.css",
        b"#status { color: rgb(255,0,0) }",
        "text/css",
        200,
    );
    let presentation = driver.until_text("stylesheet used");
    assert_eq!(
        driver
            .requests
            .values()
            .find(|request| request.head.url.ends_with("shared.css"))
            .unwrap()
            .head
            .request_id,
        request_id,
        "stylesheet made a duplicate network request"
    );
    assert!(presentation.layout.items.iter().any(|item| matches!(item,
        DisplayItem::Text { color, .. } if color.red == 255 && color.green == 0 && color.blue == 0
    )));
    driver.session.shutdown().unwrap();
}

#[test]
fn modulepreload_is_reused_by_matching_module_script() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let mut driver = Driver::new(
        "<!doctype html><head><link rel=modulepreload href=/app.mjs>
         <script type=module src=/app.mjs></script></head><body><p id=status>pending</p>",
    );
    blocked_until_request(&mut driver, "app.mjs");
    let request_id = driver
        .requests
        .values()
        .find(|request| request.head.url.ends_with("app.mjs"))
        .unwrap()
        .head
        .request_id;
    driver.respond_bytes(
        "app.mjs",
        b"document.getElementById('status').textContent='module ran';",
        "text/javascript",
        200,
    );
    driver.until_text("module ran");
    assert_eq!(
        driver
            .requests
            .values()
            .filter(|request| request.head.url.ends_with("app.mjs"))
            .count(),
        1
    );
    assert_eq!(
        driver
            .requests
            .values()
            .find(|request| request.head.url.ends_with("app.mjs"))
            .unwrap()
            .head
            .request_id,
        request_id
    );
    driver.session.shutdown().unwrap();
}

#[test]
fn failed_stylesheet_preload_retries_authoritative_stylesheet() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let mut driver = Driver::new(
        "<!doctype html><head><link rel=preload as=style href=/retry.css>\
         <link rel=stylesheet href=/retry.css></head><body><p id=status>pending</p>\
         <script>document.getElementById('status').textContent='style settled';</script>",
    );
    blocked_until_request(&mut driver, "retry.css");
    let first = driver
        .requests
        .values()
        .find(|request| request.head.url.ends_with("retry.css"))
        .unwrap()
        .head
        .request_id;
    driver.respond_bytes("retry.css", b"not found", "text/css", 404);
    driver.until_replacement_request("retry.css", first);
    driver.respond_bytes(
        "retry.css",
        b"#status { color: rgb(255,0,0) }",
        "text/css",
        200,
    );
    let presentation = driver.until_text("style settled");
    assert!(presentation.layout.items.iter().any(|item| matches!(item,
        DisplayItem::Text { color, .. } if color.red == 255 && color.green == 0 && color.blue == 0
    )));
    driver.session.shutdown().unwrap();
}

#[test]
fn wrong_mime_modulepreload_retries_the_real_module_request() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let mut driver = Driver::new(
        "<!doctype html><head><link rel=modulepreload href=/retry.mjs>\
         <script type=module src=/retry.mjs></script></head><body><p id=status>pending</p>",
    );
    blocked_until_request(&mut driver, "retry.mjs");
    let first = driver
        .requests
        .values()
        .find(|request| request.head.url.ends_with("retry.mjs"))
        .unwrap()
        .head
        .request_id;
    driver.respond_bytes("retry.mjs", b"not JavaScript", "text/plain", 200);
    driver.until_replacement_request("retry.mjs", first);
    driver.respond_bytes(
        "retry.mjs",
        b"document.getElementById('status').textContent='module retried';",
        "text/javascript",
        200,
    );
    driver.until_text("module retried");
    driver.session.shutdown().unwrap();
}

#[test]
fn parser_script_integrity_failure_prevents_execution_and_releases_parser() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let digest = base64::engine::general_purpose::STANDARD.encode(Sha256::digest(b"expected"));
    let html = format!(
        "<!doctype html><body><p id=status>pending</p>\
         <script src=/guarded.js integrity='sha256-{digest}'></script>\
         <script>document.getElementById('status').textContent=\
           window.integrityExecuted?'unsafe execution':'integrity blocked';</script>"
    );
    let mut driver = Driver::new(&html);
    blocked_until_request(&mut driver, "guarded.js");
    driver.respond("guarded.js", "window.integrityExecuted=true", 200);
    driver.until_text("integrity blocked");
    driver.session.shutdown().unwrap();
}

#[test]
fn stylesheet_integrity_failure_does_not_enter_the_cascade() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let digest = base64::engine::general_purpose::STANDARD.encode(Sha256::digest(b"expected"));
    let html = format!(
        "<!doctype html><head><link rel=stylesheet href=/guarded.css integrity='sha256-{digest}'>\
         </head><body><p id=status>pending</p><script>\
         document.getElementById('status').textContent='style checked';</script>"
    );
    let mut driver = Driver::new(&html);
    blocked_until_request(&mut driver, "guarded.css");
    driver.respond_bytes(
        "guarded.css",
        b"#status { color: rgb(255,0,0) }",
        "text/css",
        200,
    );
    let presentation = driver.until_text("style checked");
    assert!(!presentation.layout.items.iter().any(|item| matches!(item,
        DisplayItem::Text { color, .. } if color.red == 255 && color.green == 0 && color.blue == 0
    )));
    driver.session.shutdown().unwrap();
}

#[test]
fn stylesheet_integrity_success_enters_the_cascade() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let css = b"#status { color: rgb(255,0,0) }";
    let digest = base64::engine::general_purpose::STANDARD.encode(Sha256::digest(css));
    let html = format!(
        "<!doctype html><head><link rel=stylesheet href=/guarded.css integrity='sha256-{digest}'>\
         </head><body><p id=status>pending</p><script>\
         document.getElementById('status').textContent='style checked';</script>"
    );
    let mut driver = Driver::new(&html);
    blocked_until_request(&mut driver, "guarded.css");
    driver.respond_bytes("guarded.css", css, "text/css", 200);
    let presentation = driver.until_text("style checked");
    assert!(presentation.layout.items.iter().any(|item| matches!(item,
        DisplayItem::Text { color, .. } if color.red == 255 && color.green == 0 && color.blue == 0
    )));
    driver.session.shutdown().unwrap();
}

#[test]
fn successful_parser_script_sha512_executes_once_before_following_inline_script() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let code = b"window.sriExecutions=(window.sriExecutions||0)+1;";
    let digest = base64::engine::general_purpose::STANDARD.encode(Sha512::digest(code));
    let html = format!(
        "<!doctype html><body><p id=status>pending</p>\
         <script src=/verified.js integrity='sha512-{digest}'></script>\
         <script>document.getElementById('status').textContent=\
           window.sriExecutions===1?'verified once':'wrong order';</script>"
    );
    let mut driver = Driver::new(&html);
    blocked_until_request(&mut driver, "verified.js");
    driver.respond_bytes("verified.js", code, "text/javascript", 200);
    driver.until_text("verified once");
    driver.session.shutdown().unwrap();
}

#[test]
fn failed_integrity_preload_does_not_poison_the_authoritative_stylesheet() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let good_css = b"#status { color: rgb(255,0,0) }";
    let digest = base64::engine::general_purpose::STANDARD.encode(Sha256::digest(good_css));
    let html = format!(
        "<!doctype html><head><link rel=preload as=style href=/retry.css integrity='sha256-{digest}'>\
         <link rel=stylesheet href=/retry.css integrity='sha256-{digest}'></head><body><p id=status>pending</p>\
         <script>document.getElementById('status').textContent='style settled';</script>"
    );
    let mut driver = Driver::new(&html);
    blocked_until_request(&mut driver, "retry.css");
    let first = driver
        .requests
        .values()
        .find(|request| request.head.url.ends_with("retry.css"))
        .unwrap()
        .head
        .request_id;
    driver.respond_bytes(
        "retry.css",
        b"#status { color: rgb(0,0,255) }",
        "text/css",
        200,
    );
    driver.until_replacement_request("retry.css", first);
    driver.respond_bytes("retry.css", good_css, "text/css", 200);
    let presentation = driver.until_text("style settled");
    assert!(presentation.layout.items.iter().any(|item| matches!(item,
        DisplayItem::Text { color, .. } if color.red == 255 && color.green == 0 && color.blue == 0
    )));
    driver.session.shutdown().unwrap();
}
