//! Terminating a Worker between Step and Commit must release browser DB sessions.

use super::*;

#[test]
fn terminating_workers_retires_uncommitted_browser_sessions() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind Worker retirement fixture");
    let address = listener.local_addr().unwrap();
    let html = r#"<!doctype html><title>worker retirement pending</title>
        <style>body { background: rgb(220,20,20); } #state { height: 600px; }</style>
        <div id="state">pending</div><script>
        const fail = message => {
            document.title = 'worker retirement failure ' + message;
            document.getElementById('state').textContent = document.title;
        };
        const done = () => {
            document.title = 'worker retirement complete';
            document.getElementById('state').textContent = document.title;
            document.body.style.backgroundColor = 'rgb(17,170,34)';
        };
        const opened = indexedDB.open('worker-retirement', 1);
        opened.onupgradeneeded = () => opened.result.createObjectStore('items');
        opened.onerror = () => fail(opened.error?.name || 'open');
        opened.onsuccess = () => {
            const db = opened.result;
            let retired = 0;
            const verify = () => {
                const write = db.transaction('items', 'readwrite');
                write.objectStore('items').put({ answer: 42 }, 'committed');
                write.onerror = () => fail(write.error?.name || 'write');
                write.oncomplete = () => {
                    const read = db.transaction('items');
                    const staged = read.objectStore('items').get('uncommitted');
                    const committed = read.objectStore('items').get('committed');
                    read.onerror = () => fail(read.error?.name || 'read');
                    read.oncomplete = () => staged.result === undefined &&
                        committed.result?.answer === 42 ? done() : fail('rollback');
                };
            };
            const start = () => {
                const worker = new Worker('/retire-worker.js?instance=' + retired);
                worker.onerror = event => fail(event.message || 'Worker');
                worker.onmessage = event => {
                    if (event.data !== 'staged') return fail(event.data);
                    worker.terminate();
                    retired++;
                    if (retired === 2) verify();
                    else queueMicrotask(start);
                };
            };
            start();
        };
        </script>"#
        .to_string();
    // Success handlers may enqueue another request in the same transaction.
    // Keep one request pending until termination, using only the public API.
    // Native retirement tests separately cover freeing all 64 session slots.
    let worker = r#"if (typeof __hostCall !== 'undefined') throw Error('private bridge exposed');
        const opened = indexedDB.open('worker-retirement', 1);
        opened.onerror = () => postMessage(opened.error?.name || 'open');
        opened.onsuccess = () => {
            const transaction = opened.result.transaction('items', 'readwrite');
            const store = transaction.objectStore('items');
            transaction.onerror = () => postMessage(transaction.error?.name || 'transaction');
            transaction.oncomplete = () => postMessage('unexpected commit');
            let steps = 0;
            const keepActive = () => {
                if (++steps > 1024) return postMessage('termination did not arrive');
                store.get('uncommitted').onsuccess = keepActive;
            };
            store.put('staged', 'uncommitted').onsuccess = () => {
                keepActive();
                postMessage('staged');
            };
        };"#
    .to_string();
    let server = thread::spawn(move || {
        serve_parallel_fixtures(listener, 3, move |request| {
            if request.contains("GET /retire-worker.js?") {
                FixtureResponse::script(worker.clone(), Duration::ZERO)
            } else {
                FixtureResponse::html(html.clone())
            }
        })
    });
    let artifacts = TestArtifacts::new();
    let url = format!("http://{address}/page");
    let mut child = hidden_benchmark_with_fresh_profile_args(&url, &artifacts, 8000, &[]);
    let status = wait_for_child(&mut child, Duration::from_secs(30));
    let report = fs::read_to_string(&artifacts.json).unwrap_or_default();
    let served = server.join().unwrap();
    assert!(
        status.success(),
        "hidden Breeze run failed: {status}\n{report}"
    );
    assert!(report.contains("\"javascript_errors\": []"), "{report}");
    assert!(report.contains("worker retirement complete"), "{report}");
    served.expect("page and both Worker scripts must be requested");
    assert_green_capture(&artifacts, "Worker retirement did not repaint the page");
}
