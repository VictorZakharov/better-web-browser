use super::*;

const SVG: &[u8] = br#"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20"><rect width="20" height="20" fill="green"/></svg>"#;

#[test]
fn document_load_waits_for_eager_resources_including_those_added_by_dcl_and_async_callbacks() {
    let _serial = SERIAL.lock().unwrap_or_else(|error| error.into_inner());
    let mut driver = Driver::new(
        r#"<!doctype html><div id=status>pending</div>
        <img id=eager src=/first.svg><script async src=/async.js></script>
        <script>
        const status = document.getElementById('status'); const trace = [];
        const record = value => { trace.push(value); status.textContent = trace.join(' | '); };
        document.getElementById('eager').onload = () => record('image');
        document.addEventListener('DOMContentLoaded', () => {
            record('DCL');
            Promise.resolve().then(() => {
                const image = document.createElement('img'); image.src = '/late.svg';
                image.onerror = () => record('image-error'); document.body.appendChild(image);
            });
        });
        window.onload = () => record('WINDOW-LOAD');
        </script>"#,
    );
    let dcl = driver.until_text("DCL");
    assert!(!painted_text(&dcl).contains("WINDOW-LOAD"));
    driver.advance();
    driver.until_idle();
    assert!(driver.requests.keys().any(|url| url.contains("late.svg")));
    driver.respond_bytes("first.svg", SVG, "image/svg+xml", 200);
    driver.until_text("image");
    driver.respond_bytes("late.svg", b"missing", "text/plain", 404);
    let failed = driver.until_text("image-error");
    assert!(!painted_text(&failed).contains("WINDOW-LOAD"));
    driver.respond("async.js", r#"
        const link = document.createElement('link'); link.rel = 'stylesheet'; link.href = '/late.css';
        link.onload = () => record('sheet'); document.head.appendChild(link); record('async');
    "#, 200);
    driver.until_text("async");
    driver.advance();
    driver.until_idle();
    driver.respond_bytes("late.css", b"body { color: green; }", "text/css", 200);
    let loaded = driver.until_text("WINDOW-LOAD");
    let text = painted_text(&loaded);
    assert!(
        text.contains("DCL | image | image-error | async | sheet | WINDOW-LOAD"),
        "{text}"
    );
    driver.session.shutdown().unwrap();
}

#[test]
fn lazy_images_and_fetch_do_not_delay_document_load() {
    let _serial = SERIAL.lock().unwrap_or_else(|error| error.into_inner());
    let mut driver = Driver::new(
        r#"<!doctype html><div id=status>pending</div>
        <img loading=lazy src=/lazy.svg>
        <script>
          fetch('/unrelated-fetch');
          document.addEventListener('DOMContentLoaded', () => document.getElementById('status').textContent = 'DCL');
          window.onload = () => document.getElementById('status').textContent = 'DCL then WINDOW-LOAD';
        </script>"#,
    );
    driver.until_text("DCL then WINDOW-LOAD");
    assert!(driver.requests.keys().any(|url| url.contains("lazy.svg")));
    assert!(
        driver
            .requests
            .keys()
            .any(|url| url.contains("unrelated-fetch"))
    );
    driver.session.shutdown().unwrap();
}

#[test]
fn shared_lazy_and_eager_image_url_blocks_once_and_decode_error_releases_load() {
    let _serial = SERIAL.lock().unwrap_or_else(|error| error.into_inner());
    let mut driver = Driver::new(
        r#"<!doctype html><div id=status>pending</div>
        <img loading=lazy src=/shared.svg><img src=/shared.svg>
        <script>
          document.addEventListener('DOMContentLoaded', () => document.getElementById('status').textContent = 'DCL');
          window.onload = () => document.getElementById('status').textContent = 'WINDOW-LOAD';
        </script>"#,
    );
    driver.until_text("DCL");
    driver.advance();
    driver.until_idle();
    assert_eq!(driver.requests.len(), 1);
    driver.respond_bytes("shared.svg", b"not an image", "image/svg+xml", 200);
    driver.until_text("WINDOW-LOAD");
    driver.session.shutdown().unwrap();
}

#[test]
fn detached_eager_image_keeps_its_load_obligation_until_terminal_response() {
    let _serial = SERIAL.lock().unwrap_or_else(|error| error.into_inner());
    let mut driver = Driver::new(
        r#"<!doctype html><div id=status>pending</div><img id=image src=/detached.svg>
        <script>
          document.addEventListener('DOMContentLoaded', () => {
            document.getElementById('image').remove(); document.getElementById('status').textContent = 'DCL';
          });
          window.onload = () => document.getElementById('status').textContent = 'WINDOW-LOAD';
        </script>"#,
    );
    driver.until_text("DCL");
    driver.advance();
    driver.until_idle();
    driver.respond_bytes("detached.svg", SVG, "image/svg+xml", 200);
    driver.until_text("WINDOW-LOAD");
    driver.session.shutdown().unwrap();
}

#[test]
fn eager_image_admission_precedes_initial_script_removal() {
    let _serial = SERIAL.lock().unwrap_or_else(|error| error.into_inner());
    let mut driver = Driver::new(
        r#"<!doctype html><div id=status>pending</div><img id=image src=/early.svg>
        <script>
          document.getElementById('image').remove();
          document.addEventListener('DOMContentLoaded', () => document.getElementById('status').textContent = 'DCL');
          window.onload = () => document.getElementById('status').textContent = 'WINDOW-LOAD';
        </script>"#,
    );
    driver.until_text("DCL");
    driver.advance();
    driver.until_idle();
    driver.respond_bytes("early.svg", SVG, "image/svg+xml", 200);
    driver.until_text("WINDOW-LOAD");
    driver.session.shutdown().unwrap();
}
