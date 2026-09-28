//! The StorageManager estimate crosses the renderer/browser broker boundary.
use super::*;

#[test]
fn window_writes_are_visible_to_worker_storage_estimate() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind StorageManager fixture");
    let address = listener.local_addr().unwrap();
    let page = r#"<!doctype html><title>storage estimate pending</title>
        <style>body { background: rgb(220,20,20); } #state { height: 600px; }</style>
        <div id="state">pending</div><script>
        const fail = error => {
            document.title = 'storage estimate failed ' + (error?.name || error);
            document.getElementById('state').textContent = document.title;
        };
        (async () => {
            if (navigator.storage !== navigator.storage ||
                'persist' in navigator.storage || await navigator.storage.persisted())
                throw Error('StorageManager interface or mode');
            const before = await navigator.storage.estimate();
            if (before.usage !== 0 || before.quota !== 37 * 1024 * 1024)
                throw Error('initial estimate');
            localStorage.setItem('key', 'value');
            const cache = await caches.open('estimate-assets');
            await cache.put('/entry', new Response('cached body'));
            await new Promise((resolve, reject) => {
                const opened = indexedDB.open('estimate-db', 1);
                opened.onupgradeneeded = () => opened.result.createObjectStore('items');
                opened.onerror = () => reject(opened.error);
                opened.onsuccess = () => { opened.result.close(); resolve(); };
            });
            const worker = new Worker('/storage-worker.js');
            worker.onerror = event => fail(event.message || 'worker error');
            worker.onmessage = event => {
                const estimate = event.data;
                if (!estimate || estimate.usage <= before.usage ||
                    estimate.quota !== before.quota || estimate.persisted !== false)
                    return fail('worker did not see window storage');
                document.title = 'storage estimate complete';
                document.getElementById('state').textContent = document.title;
                document.body.style.backgroundColor = 'rgb(17,170,34)';
                worker.terminate();
            };
        })().catch(fail);
        </script>"#
        .to_string();
    let worker = r#"(async () => {
        const estimate = await navigator.storage.estimate();
        postMessage({usage: estimate.usage, quota: estimate.quota,
            persisted: await navigator.storage.persisted()});
    })().catch(error => postMessage({error: error.name}));"#
        .to_string();
    let server = thread::spawn(move || {
        serve_parallel_fixtures(listener, 2, move |request| {
            if request.contains("GET /storage-worker.js ") {
                FixtureResponse::script(worker.clone(), Duration::ZERO)
            } else {
                FixtureResponse::html(page.clone())
            }
        })
    });
    let artifacts = TestArtifacts::new();
    let url = format!("http://{address}/storage-estimate");
    let mut child = hidden_benchmark_with_fresh_profile_args(&url, &artifacts, 3500, &[]);
    let status = wait_for_child(&mut child, Duration::from_secs(25));
    server.join().unwrap().unwrap();
    assert!(status.success(), "hidden Breeze run failed: {status}");
    let report = fs::read_to_string(&artifacts.json).unwrap();
    assert!(report.contains("\"javascript_errors\": []"), "{report}");
    assert!(report.contains("storage estimate complete"), "{report}");
    assert_green_capture(&artifacts, "StorageManager result did not repaint the page");
}
