use super::*;

#[test]
fn relative_layer_remains_visible_above_later_app_background() {
    const HTML: &str = r#"<!doctype html><style>
      body {margin:0} #layer {position:relative;z-index:1;height:0}
      #frame {position:absolute;width:160px;height:120px;background:rgb(17,170,34)}
      #app {height:500px;background:white}
    </style><div id=layer><div id=frame></div></div><div id=app></div>
    <script>document.getElementById('frame').onclick = event => {
      if (event.isTrusted) console.log('positioned layer received click');
    };</script>"#;
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/stacking", listener.local_addr().unwrap());
    let server = thread::spawn(move || serve_fixtures(listener, 1, |_| HTML));
    let artifacts = TestArtifacts::new();
    let mut child =
        hidden_benchmark_with_args(&url, &artifacts, 1000, &["--click-after-ready", "80,60"]);
    assert!(wait_for_child(&mut child, Duration::from_secs(20)).success());
    server.join().unwrap().unwrap();
    let report = fs::read_to_string(&artifacts.json).unwrap();
    assert!(
        report.contains("positioned layer received click"),
        "{report}"
    );
    let capture = image::open(&artifacts.screenshot).unwrap().to_rgba8();
    let green = capture
        .pixels()
        .filter(|pixel| pixel[0] < 40 && pixel[1] > 130 && pixel[1] < 200 && pixel[2] < 70)
        .count();
    assert!(
        green > 1000,
        "positioned content was overpainted: {green} green pixels"
    );
}
