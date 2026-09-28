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
