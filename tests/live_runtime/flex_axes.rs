use super::*;

#[test]
fn column_drawer_keeps_content_visible_after_an_empty_spacer() {
    const HTML: &str = r#"<!doctype html><style>
      body {margin:0} aside {position:fixed;top:40px;width:200px;height:500px;
                            display:flex;flex-direction:column}
      #entry {height:60px;background:rgb(17,170,34)}
    </style><aside><div></div><nav><div id=entry>Navigation</div></nav></aside>
    <script>document.getElementById('entry').onclick = event => {
      if (event.isTrusted) console.log('drawer entry activated');
    };</script>"#;
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/drawer", listener.local_addr().unwrap());
    let server = thread::spawn(move || serve_fixtures(listener, 1, |_| HTML));
    let artifacts = TestArtifacts::new();
    let mut child =
        hidden_benchmark_with_args(&url, &artifacts, 1000, &["--click-after-ready", "80,70"]);
    assert!(wait_for_child(&mut child, Duration::from_secs(20)).success());
    server.join().unwrap().unwrap();
    let report = fs::read_to_string(&artifacts.json).unwrap();
    assert!(report.contains("drawer entry activated"), "{report}");
    let capture = image::open(&artifacts.screenshot).unwrap().to_rgba8();
    let green = capture
        .pixels()
        .filter(|pixel| pixel[0] < 40 && pixel[1] > 130 && pixel[1] < 200 && pixel[2] < 70)
        .count();
    assert!(
        green > 1000,
        "drawer content is not visible: {green} green pixels"
    );
}
