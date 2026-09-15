use super::*;

const SCRIPT: &str = r#"
    async function run() {
        // Wait for every response head before consuming any body. A blocked-body
        // network pool cannot start request 9, so it cannot complete this fixture.
        const responses = await Promise.all(Array.from({length:17}, (_, i) => fetch('/piece/' + i)));
        const lengths = await Promise.all(responses.reverse().map(async response => {
            const clone = response.clone();
            const [first, second] = await Promise.all([response.arrayBuffer(), clone.arrayBuffer()]);
            if (first.byteLength !== second.byteLength) throw new Error('clone mismatch');
            return first.byteLength;
        }));
        if (!lengths.every(length => length === 524288)) throw new Error('body length mismatch');
        return 'complete';
    }
    if (typeof document === 'undefined') {
        run().then(postMessage, error => postMessage('FAIL:' + error));
    } else {
        const worker = new Worker('/worker.js');
        const workerDone = new Promise((resolve, reject) => {
            worker.onmessage = e => e.data === 'complete' ? resolve() : reject(new Error(e.data));
            worker.onerror = e => reject(new Error(e.message));
        });
        Promise.all([run(), workerDone]).then(() => {
            document.body.style.backgroundColor = 'rgb(17, 170, 34)';
            document.title = '34 progressive response clones complete';
            worker.terminate();
        }).catch(error => console.error('progressive failure', error));
    }
"#;

#[test]
fn progressive_fetch_headers_and_clones_do_not_starve_behind_idle_bodies() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        serve_parallel_fixtures(listener, 36, |request| {
            if request.contains("GET /piece/") {
                FixtureResponse::streamed(
                    "application/octet-stream",
                    chunks(8, 65536),
                    Duration::from_millis(10),
                )
            } else if request.contains("GET /worker.js ") {
                FixtureResponse::script(SCRIPT, Duration::ZERO)
            } else {
                FixtureResponse::html(format!(
                    "<!doctype html><style>body {{ background:rgb(220,20,20); }}</style><script>{SCRIPT}</script>"
                ))
            }
        })
    });
    let artifacts = TestArtifacts::new();
    let mut child = hidden_benchmark(&format!("http://{address}/progressive"), &artifacts, 12000);
    let status = wait_for_child(&mut child, Duration::from_secs(30));
    server.join().unwrap().unwrap();
    assert!(status.success());
    let report: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&artifacts.json).unwrap()).unwrap();
    assert_eq!(report["javascript_errors"], serde_json::json!([]));
    assert_eq!(report["javascript_runtime_stopped"], false);
    assert_eq!(report["renderer_launch_errors"], serde_json::json!([]));
    assert_green_capture(
        &artifacts,
        "idle bodies starved later response heads or clone consumption",
    );
}
