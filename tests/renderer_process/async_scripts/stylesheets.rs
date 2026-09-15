use super::rendering::blocked_until_request;
use super::*;

#[test]
fn nested_imports_block_parser_script_and_first_paint_until_the_leaf_settles() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let mut driver = Driver::new(
        "<!doctype html><head><link rel=stylesheet href=/root.css></head><body><p id=status>pending</p><script>const target=document.getElementById('status'); target.textContent = 'leaf ' + getComputedStyle(target).fontWeight + ' width ' + target.getBoundingClientRect().width;</script>",
    );
    blocked_until_request(&mut driver, "root.css");
    driver.respond_bytes(
        "root.css",
        b"@import 'child.css'; p{color:rgb(4,5,6)}",
        "text/css",
        200,
    );
    blocked_until_request(&mut driver, "child.css");
    driver.respond_bytes("child.css", b"@import 'leaf.css';", "text/css", 200);
    blocked_until_request(&mut driver, "leaf.css");
    driver.session.ping(Duration::from_secs(1)).unwrap();
    driver.respond_bytes(
        "leaf.css",
        b"p{width:123px;font-weight:700}",
        "text/css",
        200,
    );
    let first = driver.until_text("leaf");
    assert!(
        painted_text(&first)
            .replace(' ', "")
            .contains("leaf700width123"),
        "{}",
        painted_text(&first)
    );
    assert!(first.layout.items.iter().any(|item| matches!(item,
        DisplayItem::Text{color,..} if color.red==4 && color.green==5 && color.blue==6)));
    driver.session.shutdown().unwrap();
}

#[test]
fn inline_import_failure_and_cycle_release_parser_and_paint() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    for failure in [false, true] {
        let mut driver = Driver::new(
            "<!doctype html><head><style>@import 'root.css'; p{color:rgb(4,5,6)}</style></head><body><p id=status>pending</p><script>document.getElementById('status').textContent='imports settled';</script>",
        );
        blocked_until_request(&mut driver, "root.css");
        driver.respond_bytes("root.css", b"@import 'child.css';", "text/css", 200);
        blocked_until_request(&mut driver, "child.css");
        driver.respond_bytes(
            "child.css",
            b"@import 'root.css#cycle';",
            "text/css",
            if failure { 404 } else { 200 },
        );
        driver.until_text("imports settled");
        assert_eq!(
            driver.requests.len(),
            2,
            "a cycle must not enqueue another download"
        );
        driver.session.shutdown().unwrap();
    }
}

#[test]
fn nonmatching_and_dynamic_links_do_not_block_parser_scripts() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    for prefix in [
        "<link media=print rel=stylesheet href=/slow.css>",
        "<link disabled rel=stylesheet href=/slow.css>",
        "<script>const sheet=document.createElement('link'); sheet.rel='stylesheet';sheet.href='/slow.css';document.head.appendChild(sheet);</script>",
    ] {
        let mut driver = Driver::new(&format!(
            "<!doctype html><head>{prefix}</head><body><p id=status>pending</p><script>document.getElementById('status').textContent='parser continued';</script>"
        ));
        driver.until_text("parser continued");
        driver.session.shutdown().unwrap();
    }
}

#[test]
fn disabling_or_changing_media_of_an_admitted_sheet_releases_parser_and_paint() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    for change in [
        "sheet.disabled=true",
        "sheet.media='print'",
        "sheet.remove()",
    ] {
        let mut driver = Driver::new(
            "<!doctype html><head><link id=sheet rel=stylesheet href=/slow.css><script async src=/release.js></script></head><body><p id=status>pending</p><script>document.getElementById('status').textContent='released';</script>",
        );
        blocked_until_request(&mut driver, "slow.css");
        blocked_until_request(&mut driver, "release.js");
        driver.respond(
            "release.js",
            &format!("const sheet=document.getElementById('sheet');{change};"),
            200,
        );
        driver.until_text("released");
        driver.session.shutdown().unwrap();
    }
}

#[test]
fn imported_css_mime_failure_errors_the_owner_once_and_releases_window_load() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let mut driver = Driver::new(
        r#"<!doctype html><body><p id=status>pending</p><script>
        const link=document.createElement('link'); link.rel='stylesheet';link.href='/root.css';
        let errors=0; link.onerror=()=>{errors++;document.getElementById('status').textContent='owner error '+errors};
        link.onload=()=>{throw Error('bad MIME import must fail its owner')};
        window.onload=()=>{document.getElementById('status').textContent='window loaded; errors '+errors};
        document.head.appendChild(link);
    </script>"#,
    );
    driver.until_text("pending");
    driver.advance();
    driver.until_idle();
    driver.respond_bytes("root.css", b"@import 'bad.css';", "text/css", 200);
    // The parent response may paint but must neither finish the owner nor window load.
    driver.until_text("pending");
    driver.advance();
    driver.until_idle();
    driver.respond_bytes("bad.css", b"p{display:none}", "text/html", 200);
    driver.until_text("window loaded; errors 1");
    driver.session.shutdown().unwrap();
}

#[test]
fn parser_created_body_link_blocks_following_script_but_not_prior_content() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let mut driver = Driver::new(
        "<!doctype html><body><p id=status>prior content</p><link rel=stylesheet href=/body.css><script>document.getElementById('status').textContent='body sheet settled';</script>",
    );
    driver.until_text("prior content");
    driver.respond_bytes("body.css", b"p{color:green}", "text/css", 200);
    driver.until_text("body sheet settled");
    driver.session.shutdown().unwrap();
}
