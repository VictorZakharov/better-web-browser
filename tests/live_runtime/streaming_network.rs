use super::*;

const STREAMING_HTML: &str = r#"<!doctype html>
<title>streaming pending</title>
<style>
  html, body { margin: 0; min-height: 100%; background: rgb(220, 20, 20); }
  #state { width: 100%; height: 600px; }
</style>
<div id="state">pending</div>
<script>
  let timerFired = false;
  setTimeout(() => { timerFired = true; }, 30);

  function completedXhr(index) {
    return new Promise((resolve, reject) => {
      const request = new XMLHttpRequest();
      const loaded = [];
      request.open('GET', '/stream/' + index);
      request.onprogress = event => {
        if (loaded.length && event.loaded <= loaded[loaded.length - 1])
          throw new Error('non-monotonic XHR progress');
        loaded.push(event.loaded);
      };
      request.onerror = reject;
      request.onload = () => {
        if (loaded.length < 2 || request.responseText.length !== 16384)
          reject(new Error('stream did not produce incremental progress'));
        else
          resolve();
      };
      request.send();
    });
  }

  function abortAndRetry() {
    return new Promise((resolve, reject) => {
      const request = new XMLHttpRequest();
      request.open('GET', '/abort');
      request.onprogress = () => request.abort();
      request.onabort = resolve;
      request.onerror = reject;
      request.send();
    }).then(() => fetch('/retry'))
      .then(response => response.arrayBuffer())
      .then(body => {
        if (body.byteLength !== 16384) throw new Error('retry body mismatch');
      });
  }

  let measuredRate = 0;
  function completedLargeBlob() {
    return new Promise((resolve, reject) => {
      const request = new XMLHttpRequest();
      const loaded = [];
      let lastLoaded = 0;
      let lastProgressAt = performance.now();
      let monotonic = true;
      request.open('GET', '/large');
      request.responseType = 'blob';
      request.onprogress = event => {
        const now = performance.now();
        monotonic = monotonic && event.loaded > lastLoaded;
        measuredRate = (event.loaded - lastLoaded) * 8000 /
          Math.max(now - lastProgressAt, 1);
        lastLoaded = event.loaded;
        lastProgressAt = now;
        loaded.push(event.loaded);
      };
      request.onerror = reject;
      request.onload = () => {
        if (
          loaded.length < 2 || !monotonic || !Number.isFinite(measuredRate) ||
          measuredRate <= 0 || request.response.size !== 25 * 1024 * 1024
        )
          reject(new Error('large streamed Blob measurement mismatch'));
        else
          resolve();
      };
      request.send();
    });
  }

  Promise.all([
    completedXhr(0), completedXhr(1), completedXhr(2), completedXhr(3),
    completedXhr(4), completedXhr(5), completedLargeBlob(), abortAndRetry()
  ]).then(() => {
    if (!timerFired) throw new Error('document event loop was blocked by response bodies');
    document.getElementById('state').textContent =
      'streaming complete ' + Math.round(measuredRate) + ' bps';
    document.body.style.backgroundColor = 'rgb(17, 170, 34)';
    document.title = 'streaming complete';
  });
</script>"#;

fn chunks(count: usize, bytes: usize) -> Vec<Vec<u8>> {
    (0..count).map(|_| vec![b'x'; bytes]).collect()
}

fn abort_chunks() -> Vec<Vec<u8>> {
    let mut body = vec![vec![b'x'; 64 * 1024]];
    body.extend(chunks(24, 1024 * 1024));
    body.push(vec![b'x'; 960 * 1024]);
    body
}

#[test]
fn eight_streams_report_progress_and_an_abort_can_retry_without_blocking() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback fixture");
    let address = listener.local_addr().expect("read fixture address");
    let server = thread::spawn(move || {
        serve_parallel_fixtures(listener, 10, |request| {
            if request.contains("GET /abort ") {
                FixtureResponse::streamed(
                    "application/octet-stream",
                    abort_chunks(),
                    Duration::from_millis(250),
                )
                .allow_disconnect()
            } else if request.contains("GET /large ") {
                FixtureResponse::streamed(
                    "application/octet-stream",
                    chunks(25, 1024 * 1024),
                    Duration::from_millis(10),
                )
            } else if request.contains("GET /stream/") || request.contains("GET /retry ") {
                FixtureResponse::streamed(
                    "application/octet-stream",
                    chunks(2, 8 * 1024),
                    Duration::from_millis(75),
                )
            } else {
                FixtureResponse::html(STREAMING_HTML)
            }
        })
    });
    let artifacts = TestArtifacts::new();
    let url = format!("http://{address}/streaming-network");

    // The CI suite runs several hidden renderers in parallel. Leave enough deterministic settle
    // headroom for the full 25 MiB stream to finish even when those processes contend for CPU.
    let mut child = hidden_benchmark(&url, &artifacts, 15000);
    let status = wait_for_child(&mut child, Duration::from_secs(35));
    server
        .join()
        .expect("fixture server panicked")
        .expect("fixture server failed");
    assert!(status.success(), "hidden Breeze run failed: {status}");

    let report = fs::read_to_string(&artifacts.json).expect("read benchmark report");
    assert!(
        report.contains("\"javascript_errors\": []"),
        "streaming fixture reported JavaScript errors:\n{report}"
    );
    assert!(
        json_integer(&report, "renderer_peak_working_set_bytes")
            .is_some_and(|bytes| bytes < 128 * 1024 * 1024),
        "streamed responses exceeded the 128 MiB renderer memory envelope:\n{report}"
    );
    assert_green_capture(
        &artifacts,
        "streamed XHR progress, abort, or retry did not complete",
    );
}
