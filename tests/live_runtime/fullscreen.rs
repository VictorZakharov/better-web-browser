use super::*;

const FULLSCREEN_FIXTURE: &str = r##"<!doctype html>
<title>fullscreen fixture</title>
<style>
  html, body { margin: 0; background-color: rgb(220, 20, 20); }
  #outside { width: 100%; height: 700px; background-color: rgb(220, 20, 20); }
  #player { width: 240px; height: 120px; background-color: rgb(17, 170, 34); color: white; }
</style>
<a id="enter" href="#fullscreen">Enter fullscreen</a>
<div id="outside">content outside the fullscreen top layer</div>
<section id="player">native fullscreen active</section>
<script>
  const player = document.querySelector('#player');
  document.querySelector('#enter').addEventListener('click', event => {
    event.preventDefault();
    player.requestFullscreen().then(
      () => console.log('__FULLSCREEN_ENTERED__'),
      error => console.error('fullscreen failed: ' + error.name)
    );
  });
</script>"##;

const FULLSCREEN_EXIT_FIXTURE: &str = r##"<!doctype html>
<title>fullscreen exit fixture</title>
<style>
  html, body { margin: 0; background-color: rgb(220, 20, 20); }
  #outside { width: 100%; height: 700px; background-color: rgb(220, 20, 20); }
  #player { width: 240px; height: 120px; background-color: rgb(17, 170, 34); color: white; }
</style>
<a id="enter" href="#fullscreen">Enter then exit fullscreen</a>
<div id="outside">restored non-fullscreen content</div>
<section id="player">temporary fullscreen content</section>
<script>
  const player = document.querySelector('#player');
  document.querySelector('#enter').addEventListener('click', event => {
    event.preventDefault();
    player.requestFullscreen()
      .then(() => document.exitFullscreen())
      .then(
        () => console.log('__FULLSCREEN_EXITED__'),
        error => console.error('fullscreen transition failed: ' + error.name)
      );
  });
</script>"##;

const FULLSCREEN_ESCAPE_FIXTURE: &str = r##"<!doctype html>
<title>fullscreen escape fixture</title>
<style>
  html, body { margin: 0; background-color: rgb(220, 20, 20); }
  #outside { width: 100%; height: 700px; background-color: rgb(220, 20, 20); }
  #player { width: 240px; height: 120px; background-color: rgb(17, 170, 34); color: white; }
</style>
<a id="enter" href="#fullscreen">Enter fullscreen</a>
<div id="outside">content restored after Escape</div>
<section id="player">press Escape to leave fullscreen</section>
<script>
  const player = document.querySelector('#player');
  document.addEventListener('fullscreenchange', () => {
    if (document.fullscreenElement) document.title = 'fullscreen entered';
    else console.log('__FULLSCREEN_ESCAPED__');
  });
  document.querySelector('#enter').addEventListener('click', event => {
    event.preventDefault();
    player.requestFullscreen().catch(error => {
      console.error('fullscreen transition failed: ' + error.name);
    });
  });
</script>"##;

const FULLSCREEN_NO_ACTIVATION_FIXTURE: &str = r#"<!doctype html>
<title>fullscreen activation policy fixture</title>
<style>html, body { margin: 0; background-color: rgb(220, 20, 20); }</style>
<script>
  document.body.requestFullscreen().then(
    () => console.error('fullscreen unexpectedly entered'),
    error => console.log('__FULLSCREEN_DENIED__' + error.name)
  );
</script>"#;

#[test]
fn trusted_link_activation_enters_native_fullscreen_in_a_hidden_window() {
    let artifacts = run_fullscreen_fixture(FULLSCREEN_FIXTURE, "__FULLSCREEN_ENTERED__");
    assert_green_capture(
        &artifacts,
        "fullscreen capture retained content outside the green top layer",
    );
}

#[test]
fn document_exit_restores_the_hidden_native_window_and_page_layout() {
    let artifacts = run_fullscreen_fixture(FULLSCREEN_EXIT_FIXTURE, "__FULLSCREEN_EXITED__");
    assert_red_capture(
        &artifacts,
        "Document.exitFullscreen did not restore the ordinary page surface",
    );
}

#[test]
fn escape_exits_page_fullscreen_before_page_keyboard_dispatch() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback fixture");
    let address = listener.local_addr().expect("read fixture address");
    let server = thread::spawn(move || serve_fixtures(listener, 1, |_| FULLSCREEN_ESCAPE_FIXTURE));
    let artifacts = TestArtifacts::new();
    let url = format!("http://{address}/fullscreen");
    let link = format!("{url}#fullscreen");
    let mut child = hidden_benchmark_with_args(
        &url,
        &artifacts,
        5_000,
        &[
            "--activate-link-after-ready",
            &link,
            "--completion-marker",
            "__FULLSCREEN_ESCAPED__",
            "--window-width",
            "800",
            "--window-height",
            "600",
        ],
    );
    if let Err(error) = super::window::escape_when_title_contains(
        &child,
        "fullscreen escape fixture",
        Duration::from_secs(1),
        Duration::from_secs(8),
    ) {
        child.kill().expect("terminate failed hidden browser");
        let _ = child.wait();
        panic!("{error}");
    }
    let status = wait_for_child(&mut child, Duration::from_secs(20));
    server
        .join()
        .expect("fixture server panicked")
        .expect("fixture server failed");
    assert!(status.success(), "hidden Breeze run failed: {status}");

    let report = fs::read_to_string(&artifacts.json).expect("read benchmark report");
    assert!(
        report.contains("log: __FULLSCREEN_ESCAPED__"),
        "Escape did not produce an acknowledged fullscreen exit:\n{report}"
    );
    assert!(
        report.contains("\"javascript_errors\": []"),
        "Escape fullscreen fixture reported JavaScript errors:\n{report}"
    );
    assert_red_capture(
        &artifacts,
        "Escape did not restore the ordinary page surface",
    );
}

#[test]
fn fullscreen_without_transient_activation_is_denied_by_the_browser() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback fixture");
    let address = listener.local_addr().expect("read fixture address");
    let server =
        thread::spawn(move || serve_fixtures(listener, 1, |_| FULLSCREEN_NO_ACTIVATION_FIXTURE));
    let artifacts = TestArtifacts::new();
    let url = format!("http://{address}/fullscreen");
    let mut child = hidden_benchmark_with_args(
        &url,
        &artifacts,
        5_000,
        &[
            "--completion-marker",
            "__FULLSCREEN_DENIED__NotAllowedError",
            "--window-width",
            "800",
            "--window-height",
            "600",
        ],
    );
    let status = wait_for_child(&mut child, Duration::from_secs(20));
    server
        .join()
        .expect("fixture server panicked")
        .expect("fixture server failed");
    assert!(status.success(), "hidden Breeze run failed: {status}");

    let report = fs::read_to_string(&artifacts.json).expect("read benchmark report");
    assert!(
        report.contains("log: __FULLSCREEN_DENIED__NotAllowedError"),
        "browser did not reject fullscreen without transient activation:\n{report}"
    );
    assert!(
        report.contains("\"javascript_errors\": []"),
        "activation-policy fixture reported JavaScript errors:\n{report}"
    );
    assert_red_capture(
        &artifacts,
        "denied fullscreen request changed the ordinary page surface",
    );
}

fn run_fullscreen_fixture(fixture: &'static str, marker: &str) -> TestArtifacts {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback fixture");
    let address = listener.local_addr().expect("read fixture address");
    let server = thread::spawn(move || serve_fixtures(listener, 1, |_| fixture));
    let artifacts = TestArtifacts::new();
    let url = format!("http://{address}/fullscreen");
    let link = format!("{url}#fullscreen");
    let mut child = hidden_benchmark_with_args(
        &url,
        &artifacts,
        5_000,
        &[
            "--activate-link-after-ready",
            &link,
            "--completion-marker",
            marker,
            "--window-width",
            "800",
            "--window-height",
            "600",
        ],
    );
    let status = wait_for_child(&mut child, Duration::from_secs(20));
    server
        .join()
        .expect("fixture server panicked")
        .expect("fixture server failed");
    assert!(status.success(), "hidden Breeze run failed: {status}");

    let report = fs::read_to_string(&artifacts.json).expect("read benchmark report");
    assert!(
        report.contains(&format!("log: {marker}")),
        "fullscreen Promise did not settle after native acknowledgement for {marker}:\n{report}"
    );
    assert!(
        report.contains("\"javascript_errors\": []"),
        "fullscreen fixture reported JavaScript errors:\n{report}"
    );
    artifacts
}

fn assert_red_capture(artifacts: &TestArtifacts, message: &str) {
    let capture = image::open(&artifacts.screenshot)
        .expect("open benchmark capture")
        .to_rgba8();
    let pixel = capture.get_pixel(capture.width() / 2, capture.height() / 2);
    assert!(
        pixel[0] > 150 && pixel[1] < 60 && pixel[2] < 60,
        "{message}; center pixel was {pixel:?}"
    );
}
