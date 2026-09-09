use super::*;

#[test]
fn deferred_list_waits_in_order_while_async_and_first_presentation_proceed() {
    let _serial = SERIAL.lock().unwrap_or_else(|error| error.into_inner());
    let mut driver = Driver::new(
        r#"<!doctype html><div id=status>initial content</div>
        <script>
          window.trace=[]; window.mark=x=>{trace.push(x); document.querySelector('#status').textContent=trace.join('|')};
          document.addEventListener('DOMContentLoaded',()=>mark('DCL'));
          window.addEventListener('load',()=>mark('LOAD'));
        </script>
        <script id=first defer src=/first.js></script>
        <script type=module src=/second.js></script>
        <script async src=/fast.js></script>
        <script>document.querySelector('#first').onload=()=>mark('first-load');</script>"#,
    );
    driver.until_text("initial content");
    driver.session.ping(Duration::from_secs(1)).unwrap();
    driver.respond(
        "second.js",
        "import './dependency.js'; mark('second');",
        200,
    );
    until_request(&mut driver, "dependency.js");
    driver.respond("dependency.js", "mark('dependency');", 200);
    driver.respond("fast.js", "mark('fast');", 200);
    let fast = driver.until_text("fast");
    assert!(
        !painted_text(&fast).contains("dependency"),
        "preparation must not evaluate"
    );
    driver.advance();
    driver.until_idle();
    driver.respond(
        "first.js",
        "mark('first-'+document.readyState); Promise.resolve().then(()=>mark('micro'));",
        200,
    );
    driver.until_text("fast|first-interactive|micro|first-load|dependency|second|DCL|LOAD");
    driver.session.shutdown().unwrap();
}

#[test]
fn deferred_failure_releases_order_and_prepared_detached_owners_keep_their_source() {
    let _serial = SERIAL.lock().unwrap_or_else(|error| error.into_inner());
    let mut driver = Driver::new(
        r#"<!doctype html><div id=status>pending</div>
        <script>window.trace=[];window.mark=x=>{trace.push(x);document.querySelector('#status').textContent=trace.join('|')};
        document.addEventListener('DOMContentLoaded',()=>mark('DCL'));</script>
        <script id=bad defer src=/bad.js></script><script id=a defer src=/shared.js></script>
        <script id=b defer src=/shared.js></script>
        <script>document.querySelector('#bad').onerror=e=>mark('error-'+e.isTrusted);
        const a=document.querySelector('#a'); a.remove(); a.src='/replacement.js';</script>"#,
    );
    driver.until_text("pending");
    driver.respond("shared.js", "mark(document.currentScript.id);", 200);
    driver.advance();
    driver.until_idle();
    driver.respond("bad.js", "not JavaScript", 404);
    driver.until_text("error-true|a|b|DCL");
    assert!(
        !driver
            .requests
            .keys()
            .any(|url| url.contains("replacement.js"))
    );
    driver.session.shutdown().unwrap();
}

#[test]
fn cyclic_shared_module_graph_is_prepared_without_blocking_and_evaluated_once() {
    let _serial = SERIAL.lock().unwrap_or_else(|error| error.into_inner());
    let mut driver = Driver::new(
        r#"<!doctype html><div id=status>pending</div>
        <script>window.trace=[];window.shared=0;window.mark=x=>{trace.push(x);document.querySelector('#status').textContent=trace.join('|')};
        document.addEventListener('DOMContentLoaded',()=>mark('DCL-'+shared));
        window.addEventListener('load',()=>mark('LOAD'));</script>
        <script id=inline type=module>import {a} from './a.js';mark('first-'+a());await new Promise(()=>{});mark('never');</script>
        <script type=module>import {a} from './a.js';mark('second-'+a());</script>
        <script>document.querySelector('#inline').onload=()=>mark('incorrect-inline-load');</script>"#,
    );
    driver.until_text("pending");
    driver.respond(
        "a.js",
        "import {b} from './b.js'; export function a(){return b} shared++;",
        200,
    );
    until_request(&mut driver, "b.js");
    driver.session.ping(Duration::from_secs(1)).unwrap();
    driver.respond("b.js", "import {a} from './a.js'; export const b=7;", 200);
    driver.until_text("first-7|second-7|DCL-1|LOAD");
    driver.session.shutdown().unwrap();
}

#[test]
fn failed_module_dependency_errors_each_owner_and_unblocks_dom_content_loaded() {
    let _serial = SERIAL.lock().unwrap_or_else(|error| error.into_inner());
    let mut driver = Driver::new(
        r#"<!doctype html><div id=status>pending</div>
        <script>window.trace=[];window.mark=x=>{trace.push(x);document.querySelector('#status').textContent=trace.join('|')};
        document.addEventListener('DOMContentLoaded',()=>mark('DCL'));</script>
        <script id=a type=module src=/root.js></script><script id=b type=module src=/root.js></script>
        <script>for(const id of ['a','b'])document.getElementById(id).onerror=e=>mark(id+'-'+e.isTrusted);</script>"#,
    );
    driver.until_text("pending");
    driver.respond(
        "root.js",
        "import './wrong-mime.js';mark('must-not-run');",
        200,
    );
    until_request(&mut driver, "wrong-mime.js");
    driver.respond_bytes("wrong-mime.js", b"export const value=1;", "text/plain", 200);
    driver.until_text("a-true|b-true|DCL");
    driver.session.shutdown().unwrap();
}

fn until_request(driver: &mut Driver, suffix: &str) {
    while !driver.requests.keys().any(|url| url.ends_with(suffix)) {
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
                if update.next_timer_micros.is_some() {
                    driver.advance()
                }
            }
            RendererEvent::Presentation(frame) => {
                if frame.next_timer_micros.is_some() {
                    driver.advance()
                }
            }
            RendererEvent::Diagnostic { .. } => (),
            event => panic!("unexpected event while waiting for {suffix}: {event:?}"),
        }
    }
}
