use super::*;

#[test]
fn parser_handlers_receive_native_input_and_window_load() {
    const HTML: &str = include_str!("../../benchmarks/alpha/fixtures/inline-event-handlers.html");
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/inline-handlers", listener.local_addr().unwrap());
    let server = thread::spawn(move || serve_fixtures(listener, 1, |_| HTML));
    let artifacts = TestArtifacts::new();
    let mut child = hidden_benchmark_with_args(
        &url,
        &artifacts,
        1200,
        &[
            "--activate-selector-after-ready",
            "#action",
            "--navigation-delay-ms",
            "350",
        ],
    );
    assert!(wait_for_child(&mut child, Duration::from_secs(20)).success());
    server.join().unwrap().unwrap();
    let report = fs::read_to_string(&artifacts.json).unwrap();
    assert!(report.contains("inline-handlers"), "{report}");
    assert!(report.contains(r#"\"good\":true"#), "{report}");
}

#[test]
fn parser_merges_new_body_attributes_without_reactivating_unchanged_handlers() {
    const HTML: &str = r#"<!doctype html><body onclick="throw new Error('replayed')">
      <script>
        window.results=[]; document.body.onclick=null;
        document.body.onmousedown=()=>results.push('old');
        document.body.addEventListener('mousedown',()=>results.push('listener'));
      </script>
      <body onmousedown="results.push('merged')">
      <p>Continue parsing</p><script>
        document.body.click();
        document.body.dispatchEvent(new Event('mousedown'));
        document.title=results.join(',');
      </script>"#;
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/merged-body", listener.local_addr().unwrap());
    let server = thread::spawn(move || serve_fixtures(listener, 1, |_| HTML));
    let artifacts = TestArtifacts::new();
    let mut child = hidden_benchmark(&url, &artifacts, 800);
    assert!(wait_for_child(&mut child, Duration::from_secs(20)).success());
    server.join().unwrap().unwrap();
    let report: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&artifacts.json).unwrap()).unwrap();
    assert_eq!(
        report["titles"]["document_title"], "merged,listener",
        "{report}"
    );
    assert!(
        report["javascript_errors"].as_array().unwrap().is_empty(),
        "{report}"
    );
}
