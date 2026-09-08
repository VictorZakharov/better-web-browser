use super::*;

#[test]
fn repeated_navigation_keeps_keyboard_mse_controls_and_fullscreen_healthy() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback fixture");
    let address = listener.local_addr().expect("read fixture address");
    let media_base64 = include_str!("../fixtures/media/test-1s.mp4.base64")
        .chars()
        .filter(|character| !character.is_ascii_whitespace())
        .collect::<String>();
    let fixture = format!(
        r#"<!doctype html><title>media controls</title>
        <style>html,body{{margin:0;background:#111}}video{{width:640px;height:360px}}</style>
        <video id="movie" muted></video><output id="state">loading</output><script>
        const source = new MediaSource();
        const note = value => console.log('__MEDIA_CONTROL__' + value);
        for (const name of ['play', 'pause', 'seeked', 'volumechange']) {{
          movie.addEventListener(name, () => note(name));
        }}
        document.addEventListener('fullscreenchange', () => note('fullscreen'));
        source.addEventListener('sourceopen', () => {{
          const buffer = source.addSourceBuffer('video/mp4; codecs="avc1.42E01E,mp4a.40.2"');
          buffer.addEventListener('updateend', () => source.endOfStream(), {{once:true}});
          const binary = atob('{media_base64}');
          buffer.appendBuffer(Uint8Array.from(binary, value => value.charCodeAt(0)));
        }}, {{once:true}});
        source.addEventListener('sourceended', () => {{ state.textContent = 'ready'; }});
        document.addEventListener('keydown', event => {{
          if (event.key === 'k') {{
            if (movie.paused) movie.play(); else movie.pause();
          }} else if (event.key === 'ArrowRight') {{
            movie.currentTime = Math.min(movie.duration, movie.currentTime + 0.25);
          }} else if (event.key === 'm') {{
            movie.muted = !movie.muted;
            movie.volume = 0.5;
          }} else if (event.key === 'f') {{
            movie.requestFullscreen();
          }}
        }});
        movie.src = URL.createObjectURL(source);
        </script>"#
    );
    let server = thread::spawn(move || {
        serve_parallel_fixtures(listener, 4, move |_| {
            FixtureResponse::html(fixture.clone()).header("Cache-Control", "no-store")
        })
    });
    let artifacts = TestArtifacts::new();
    let url = format!("http://{address}/media-controls");
    let mut child = hidden_benchmark_with_args(
        &url,
        &artifacts,
        200,
        &[
            "--key-after-ready",
            "k,KeyK",
            "--key-after-ready",
            "k,KeyK",
            "--key-after-ready",
            "ArrowRight,ArrowRight",
            "--navigate-after-ready",
            &url,
            "--key-after-ready",
            "k,KeyK",
            "--key-after-ready",
            "k,KeyK",
            "--key-after-ready",
            "ArrowRight,ArrowRight",
            "--navigate-after-ready",
            &url,
            "--key-after-ready",
            "k,KeyK",
            "--key-after-ready",
            "k,KeyK",
            "--key-after-ready",
            "ArrowRight,ArrowRight",
            "--navigate-after-ready",
            &url,
            "--key-after-ready",
            "k,KeyK",
            "--key-after-ready",
            "k,KeyK",
            "--key-after-ready",
            "ArrowRight,ArrowRight",
            "--key-after-ready",
            "m,KeyM",
            "--key-after-ready",
            "f,KeyF",
            "--key-after-ready",
            "k,KeyK",
            "--diagnostic-selector",
            "video:fullscreen",
            "--navigation-delay-ms",
            "300",
        ],
    );
    let status = wait_for_child(&mut child, Duration::from_secs(30));
    server
        .join()
        .expect("fixture server panicked")
        .expect("fixture server failed");
    assert!(status.success(), "hidden Breeze run failed: {status}");

    let report = fs::read_to_string(&artifacts.json).expect("read benchmark report");
    let json: serde_json::Value = serde_json::from_str(&report).expect("parse benchmark report");
    for event in ["play", "pause", "seeked", "volumechange", "fullscreen"] {
        assert!(
            report.contains(&format!("__MEDIA_CONTROL__{event}")),
            "missing {event} control acknowledgement:\n{report}"
        );
    }
    assert_eq!(
        json["diagnostics"][0]["total_matches"], 1,
        "video did not enter native fullscreen:\n{report}"
    );
    assert_eq!(json["media"]["playing"], true, "final play did not settle");
    assert!(
        json["renderer_exits"].as_array().is_some_and(Vec::is_empty),
        "navigation endurance loop lost a renderer: {}",
        json["renderer_exits"]
    );
    assert_eq!(
        json["process_count"], 2,
        "navigation leaked a renderer or media session"
    );
    assert!(
        json["media"]["current_time_seconds"]
            .as_f64()
            .unwrap_or_default()
            >= 0.25,
        "seek did not advance the media clock: {}",
        json["media"]
    );
    assert!(
        json["javascript_errors"]
            .as_array()
            .is_some_and(Vec::is_empty),
        "media controls produced JavaScript errors:\n{report}"
    );
}
