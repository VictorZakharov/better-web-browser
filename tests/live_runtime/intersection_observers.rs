use super::support::*;
use std::{fs, net::TcpListener, thread, time::Duration};

#[test]
fn intersection_observers_use_clipped_geometry_and_queued_snapshots() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!(
        "http://{}/intersection-observers.html",
        listener.local_addr().unwrap()
    );
    let server = thread::spawn(move || {
        serve_parallel_fixtures(listener, 1, |_| {
            FixtureResponse::html(include_str!(
                "../../benchmarks/alpha/fixtures/intersection-observers.html"
            ))
        })
    });
    let artifacts = TestArtifacts::new();
    let mut child = hidden_benchmark_with_fresh_profile_args(&url, &artifacts, 1800, &[]);
    assert!(wait_for_child(&mut child, Duration::from_secs(20)).success());
    server.join().unwrap().unwrap();
    let report: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&artifacts.json).unwrap()).unwrap();
    assert_eq!(
        report["titles"]["document_title"], "Intersection observers PASS",
        "{report}"
    );
    assert_eq!(report["javascript_runtime_stopped"], false, "{report}");
    assert!(
        report["javascript_errors"].as_array().unwrap().is_empty(),
        "{report}"
    );
}
