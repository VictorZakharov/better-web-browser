//! CacheStorage must cross renderer/broker boundaries and survive browser restart.
use super::*;

#[test]
fn cache_storage_persists_after_hidden_browser_restarts() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind CacheStorage fixture");
    let address = listener.local_addr().unwrap();
    let write = r#"<!doctype html><title>cache write pending</title><script>
        (async () => {
            const cache = await caches.open('restart-test');
            await cache.put('/entry', new Response('persisted body'));
            document.title = 'cache write complete';
        })().catch(error => { document.title = 'cache write failed ' + error.name; });
        </script>"#
        .to_string();
    let read = r#"<!doctype html><title>cache read pending</title><script>
        (async () => {
            const cache = await caches.open('restart-test');
            const response = await cache.match('/entry');
            document.title = response && await response.text() === 'persisted body' ?
                'cache persisted across restart' : 'cache entry lost';
        })().catch(error => { document.title = 'cache read failed ' + error.name; });
        </script>"#
        .to_string();
    let server = thread::spawn(move || {
        serve_parallel_fixtures(listener, 2, move |request| {
            FixtureResponse::html(if request.contains("GET /write") {
                write.clone()
            } else {
                read.clone()
            })
        })
    });
    let artifacts = TestArtifacts::new();
    let write_url = format!("http://{address}/write");
    let mut first = hidden_benchmark_with_reused_profile(&write_url, &artifacts, 2500);
    assert!(wait_for_child(&mut first, Duration::from_secs(25)).success());
    let first_report = fs::read_to_string(&artifacts.json).unwrap();
    assert!(
        first_report.contains("cache write complete"),
        "{first_report}"
    );
    let read_url = format!("http://{address}/read");
    let mut second = hidden_benchmark_with_reused_profile(&read_url, &artifacts, 2500);
    assert!(wait_for_child(&mut second, Duration::from_secs(25)).success());
    server.join().unwrap().unwrap();
    let second_report = fs::read_to_string(&artifacts.json).unwrap();
    assert!(
        second_report.contains("\"javascript_errors\": []"),
        "{second_report}"
    );
    assert!(
        second_report.contains("cache persisted across restart"),
        "{second_report}"
    );
}

#[test]
fn dedicated_worker_and_window_share_browser_owned_cache_storage() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind CacheStorage fixture");
    let address = listener.local_addr().unwrap();
    let page = r#"<!doctype html><title>worker cache pending</title>
        <style>body { background: rgb(220,20,20); } #state { height: 600px; }</style>
        <div id="state">pending</div><script>
        const fail = error => {
            document.title = 'worker cache failed ' + (error?.name || error);
            document.getElementById('state').textContent = document.title;
        };
        const worker = new Worker('/cache-worker.js');
        worker.onerror = event => fail(event.message || 'worker error');
        worker.onmessage = async event => {
            try {
                if (event.data === 'worker-wrote') {
                    const response = await caches.match('/from-worker');
                    if (!response || await response.text() !== 'worker body')
                        throw Error('window could not read worker entry');
                    const cache = await caches.open('worker-shared');
                    await cache.put('/from-window', new Response('window body'));
                    worker.postMessage('read-window');
                } else if (event.data === 'worker-read-window') {
                    document.title = 'worker cache complete';
                    document.getElementById('state').textContent = document.title;
                    document.body.style.backgroundColor = 'rgb(17,170,34)';
                    worker.terminate();
                } else fail('unexpected worker message: ' + event.data);
            } catch (error) { fail(error); }
        };
        </script>"#
        .to_string();
    let worker = r#"(async () => {
        const cache = await caches.open('worker-shared');
        await cache.put('/from-worker', new Response('worker body'));
        postMessage('worker-wrote');
        onmessage = async () => {
            try {
                const response = await caches.match('/from-window');
                if (!response || await response.text() !== 'window body')
                    throw Error('worker could not read window entry');
                postMessage('worker-read-window');
            } catch (error) { postMessage('failure:' + error); }
        };
    })().catch(error => postMessage('failure:' + error));"#
        .to_string();
    let server = thread::spawn(move || {
        serve_parallel_fixtures(listener, 2, move |request| {
            if request.contains("GET /cache-worker.js ") {
                FixtureResponse::script(worker.clone(), Duration::ZERO)
            } else {
                FixtureResponse::html(page.clone())
            }
        })
    });
    let artifacts = TestArtifacts::new();
    let url = format!("http://{address}/worker-cache");
    let mut child = hidden_benchmark_with_fresh_profile_args(&url, &artifacts, 3500, &[]);
    let status = wait_for_child(&mut child, Duration::from_secs(25));
    server.join().unwrap().unwrap();
    assert!(status.success(), "hidden Breeze run failed: {status}");
    let report = fs::read_to_string(&artifacts.json).unwrap();
    assert!(report.contains("\"javascript_errors\": []"), "{report}");
    assert!(report.contains("worker cache complete"), "{report}");
    assert_green_capture(&artifacts, "Worker CacheStorage did not repaint the page");
}
