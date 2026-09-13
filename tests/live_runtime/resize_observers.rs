use super::*;

#[test]
fn resize_observer_responsive_component_passes_in_hidden_browser() {
    const HTML: &str = include_str!("../../benchmarks/alpha/fixtures/resize-observer.html");
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/resize-observer", listener.local_addr().unwrap());
    let server = thread::spawn(move || serve_fixtures(listener, 1, |_| HTML));
    let artifacts = TestArtifacts::new();
    let mut child = hidden_benchmark(&url, &artifacts, 1200);
    assert!(wait_for_child(&mut child, Duration::from_secs(20)).success());
    server.join().unwrap().unwrap();
    let report: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&artifacts.json).unwrap()).unwrap();
    assert_eq!(
        report["javascript_errors"],
        serde_json::json!([]),
        "{report}"
    );
    assert!(
        report["javascript_console"]
            .as_array()
            .unwrap()
            .contains(&serde_json::json!("log: resize-fixture:PASS")),
        "{report}"
    );
}
