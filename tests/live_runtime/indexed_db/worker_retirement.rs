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
    // The internal host bridge pins an otherwise public readwrite transaction
    // after Step, before the JS transaction's automatic Commit. This is a test
    // probe, not a page-facing IndexedDB alternative or production hook.
    let worker = r#"const total = 32;
        const inFlight = new Set();
        let next = 1;
        let completed = 0;
        const receive = globalThis.__receiveDatabaseEvent;
        globalThis.__receiveDatabaseEvent = (incoming, payload) => {
            if (!inFlight.delete(Number(incoming))) return receive(incoming, payload);
            const result = JSON.parse(payload);
            if (result.kind !== 'transaction')
                return postMessage('Step failed: ' + result.name);
            completed++;
            if (completed === total) postMessage('staged');
            else issue();
        };
        function issue() {
            while (next <= total && inFlight.size < 4) {
                const command = {
                    kind: 'transaction', phase: 'step', transactionId: next++,
                    name: 'worker-retirement', version: 1, mode: 'readwrite',
                    operations: [{ kind: 'put', store: 'items',
                        key: { type: 'String', value: 'uncommitted' },
                        value: '"staged"', overwrite: true }]
                };
                inFlight.add(Number(__hostCall('databaseRequest', JSON.stringify(command))));
            }
        }
        issue();"#
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
    server.join().unwrap().unwrap();
    assert!(status.success(), "hidden Breeze run failed: {status}");
    let report = fs::read_to_string(&artifacts.json).unwrap();
    assert!(report.contains("\"javascript_errors\": []"), "{report}");
    assert!(report.contains("worker retirement complete"), "{report}");
    assert_green_capture(&artifacts, "Worker retirement did not repaint the page");
}
