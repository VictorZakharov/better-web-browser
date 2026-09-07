use super::*;

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
