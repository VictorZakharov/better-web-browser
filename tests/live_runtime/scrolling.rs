use super::*;

#[test]
fn script_scroll_request_moves_native_pixels_and_uses_absolute_overflow_extent() {
    const HTML: &str = r#"<!doctype html><title>script scroll fixture</title>
        <style>html,body{margin:0}main{position:absolute;top:1000px;width:600px;
        height:1000px;background:rgb(17,170,34)}</style><main>Script-scrolled content</main>
        <script>setTimeout(() => {
            document.scrollingElement.scrollTop = 1000;
            console.log('script requested scroll:' + scrollY);
        }, 1700);</script>"#;
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/script-scroll", listener.local_addr().unwrap());
    let server = thread::spawn(move || serve_fixtures(listener, 1, |_| HTML));
    let artifacts = TestArtifacts::new();
    let mut child = hidden_benchmark(&url, &artifacts, 2500);
    assert!(wait_for_child(&mut child, Duration::from_secs(20)).success());
    server.join().unwrap().unwrap();
    let report = fs::read_to_string(&artifacts.json).unwrap();
    assert!(report.contains("script requested scroll:1000"), "{report}");
    assert!(report.contains("\"javascript_errors\": []"), "{report}");
    let capture = image::open(&artifacts.screenshot).unwrap().to_rgba8();
    let green = capture
        .pixels()
        .filter(|p| p[0] < 40 && p[1] > 130 && p[1] < 200 && p[2] < 70)
        .count();
    assert!(
        green > 40_000,
        "scroll changed script state but not native painting: {green} green pixels"
    );
}

#[test]
fn delayed_hidden_scroll_actions_use_native_offsets_and_deliver_ordered_scroll_events() {
    const HTML: &str = r#"<!doctype html><title>delayed scroll fixture</title>
        <style>html,body{margin:0}main{height:3000px}</style><main>Scrollable document</main>
        <script>
          const positions = [];
          addEventListener('scroll', () => {
            positions.push(scrollY);
            console.log('native scroll sequence:' + positions.join(','));
          });
        </script>"#;
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/delayed-scroll", listener.local_addr().unwrap());
    let server = thread::spawn(move || serve_fixtures(listener, 1, |_| HTML));
    let artifacts = TestArtifacts::new();
    let mut child = hidden_benchmark_with_args(
        &url,
        &artifacts,
        500,
        &[
            "--scroll-after-ready",
            "800",
            "--scroll-after-ready",
            "0",
            "--scroll-after-ready",
            "2147483647",
            "--navigation-delay-ms",
            "500",
        ],
    );
    let status = wait_for_child(&mut child, Duration::from_secs(20));
    server.join().unwrap().unwrap();
    assert!(status.success(), "hidden browser failed: {status}");
    let report: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&artifacts.json).unwrap()).unwrap();
    assert!(report["error"].is_null(), "hidden scroll failed: {report}");
    assert_eq!(report["javascript_errors"], serde_json::json!([]));
    let positions = report["javascript_console"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(serde_json::Value::as_str)
        .filter_map(|line| {
            line.split_once("native scroll sequence:")
                .map(|(_, value)| value)
        })
        .next_back()
        .expect("native scroll events were not delivered")
        .split(',')
        .map(|value| value.parse::<f64>().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(
        positions.len(),
        3,
        "scroll actions lost their order: {positions:?}"
    );
    assert!(
        (positions[0] - 800.0).abs() <= 1.0,
        "CSS/native scale mismatch: {positions:?}"
    );
    assert_eq!(positions[1], 0.0);
    assert!(
        (2000.0..3000.0).contains(&positions[2]),
        "scroll was not clamped to the document: {positions:?}"
    );
}

#[test]
fn native_wheel_reaches_absolute_document_overflow_and_delivers_scroll_event() {
    const HTML: &str = r#"<!doctype html><title>absolute scroll fixture</title>
        <style>body{margin:0} main{position:absolute;top:0;width:600px;height:1800px;
        background:linear-gradient(red,green)}</style><main>Scrollable application</main>
        <script>addEventListener('scroll', () => {
            if (scrollY > 500) console.log('native wheel reached overflow');
        });</script>"#;
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/overflow", listener.local_addr().unwrap());
    let server = thread::spawn(move || serve_fixtures(listener, 1, |_| HTML));
    let artifacts = TestArtifacts::new();
    let mut child = hidden_benchmark(&url, &artifacts, 2500);
    window::wheel_when_title_contains(&child, "absolute scroll fixture", -1200);
    let status = wait_for_child(&mut child, Duration::from_secs(20));
    server.join().unwrap().unwrap();
    assert!(status.success(), "hidden browser failed: {status}");
    let report = fs::read_to_string(&artifacts.json).unwrap();
    assert!(
        report.contains("native wheel reached overflow"),
        "wheel did not scroll: {report}"
    );
    assert!(
        report.contains("\"javascript_errors\": []"),
        "script failure: {report}"
    );
}
