use super::support::*;
use std::{fs, net::TcpListener, thread, time::Duration};

#[test]
fn dynamic_modules_fetch_without_blocking_and_settle_native_import_promises() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!(
        "http://{}/dynamic-modules.html",
        listener.local_addr().unwrap()
    );
    let server = thread::spawn(move || {
        serve_parallel_fixtures(listener, 5, |request| {
            if request.contains("GET /module-shared.js?") {
                FixtureResponse::script(
                    include_str!("../../benchmarks/alpha/fixtures/module-shared.js"),
                    Duration::from_millis(120),
                )
            } else if request.contains("GET /module-await.js ") {
                FixtureResponse::script(
                    include_str!("../../benchmarks/alpha/fixtures/module-await.js"),
                    Duration::ZERO,
                )
            } else if request.contains("GET /module-throws.js ") {
                FixtureResponse::script(
                    include_str!("../../benchmarks/alpha/fixtures/module-throws.js"),
                    Duration::ZERO,
                )
            } else if request.contains("GET /module-absent.js ") {
                FixtureResponse::html("not JavaScript")
            } else {
                FixtureResponse::html(include_str!(
                    "../../benchmarks/alpha/fixtures/dynamic-modules.html"
                ))
            }
        })
    });
    let artifacts = TestArtifacts::new();
    let mut child = hidden_benchmark(&url, &artifacts, 1200);
    assert!(wait_for_child(&mut child, Duration::from_secs(20)).success());
    server.join().unwrap().unwrap();
    let report: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&artifacts.json).unwrap()).unwrap();
    assert_eq!(
        report["titles"]["document_title"], "Dynamic modules PASS",
        "{report}"
    );
    assert_eq!(report["javascript_runtime_stopped"], false, "{report}");
    assert!(
        report["javascript_errors"].as_array().unwrap().is_empty(),
        "{report}"
    );
}

#[test]
fn dynamic_import_idle_wakeup_redirect_base_and_failed_fetch_retry() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/", listener.local_addr().unwrap());
    let server = thread::spawn(move || {
        let retries = AtomicUsize::new(0);
        serve_parallel_fixtures(listener, 6, move |request| {
            if request.contains("GET /redirect.js ") {
                FixtureResponse::script("", Duration::ZERO)
                    .status(302, "Found")
                    .header("Location", "/assets/main.js")
            } else if request.contains("GET /assets/main.js ") {
                FixtureResponse::script(
                    "import {answer} from './dep.js';export {answer};export const base=import.meta.url;export const later=()=>import('./dep.js');",
                    Duration::ZERO,
                )
            } else if request.contains("GET /assets/dep.js ") {
                FixtureResponse::script("export const answer=42", Duration::ZERO)
            } else if request.contains("GET /retry.js ") {
                if retries.fetch_add(1, Ordering::SeqCst) == 0 {
                    FixtureResponse::html("not found").status(404, "Not Found")
                } else {
                    FixtureResponse::resource(
                        "\u{feff}export const ok='🙂'",
                        "text/javascript; charset=windows-1252",
                        Duration::ZERO,
                    )
                }
            } else {
                FixtureResponse::html(
                    r#"<title>waiting</title><script>
                  import('./redirect.js').then(async m=>{
                    if(m.answer!==42 || !m.base.endsWith('/assets/main.js') || (await m.later()).answer!==42)throw Error('base');
                    await import('./retry.js').then(()=>{throw Error('unexpected success')}, e=>{if(!(e instanceof TypeError))throw e});
                    if((await import('./retry.js')).ok!=='🙂')throw Error('retry encoding');
                    document.title='Redirect and retry PASS';
                  }).catch(e=>document.title='FAIL '+e);
                </script>"#,
                )
            }
        })
    });
    let artifacts = TestArtifacts::new();
    let mut child = hidden_benchmark(&url, &artifacts, 1000);
    assert!(wait_for_child(&mut child, Duration::from_secs(20)).success());
    server.join().unwrap().unwrap();
    let report: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&artifacts.json).unwrap()).unwrap();
    assert_eq!(
        report["titles"]["document_title"], "Redirect and retry PASS",
        "{report}"
    );
    assert!(
        report["javascript_errors"].as_array().unwrap().is_empty(),
        "{report}"
    );
}
