use super::*;

#[test]
fn authored_flex_button_paints_children_and_delivers_trusted_submit() {
    const HTML: &str = r#"<!doctype html><style>
      body { margin:0; } button { display:flex; width:200px; height:80px; padding:0;
      border:0; background:white; } span { display:block; width:80px; height:80px;
      background:rgb(17,170,34); }
    </style><form><button name='action' value='save'><span></span>Save</button></form>
    <script>document.querySelector('form').addEventListener('submit', event => {
      event.preventDefault(); if(event.isTrusted) console.log('authored submit reached');
    });</script>"#;
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/button", listener.local_addr().unwrap());
    let server = thread::spawn(move || serve_fixtures(listener, 1, |_| HTML));
    let artifacts = TestArtifacts::new();
    let mut child = hidden_benchmark_with_args(
        &url,
        &artifacts,
        1200,
        &["--activate-selector-after-ready", "button"],
    );
    assert!(wait_for_child(&mut child, Duration::from_secs(20)).success());
    server.join().unwrap().unwrap();
    let report = fs::read_to_string(&artifacts.json).unwrap();
    assert!(report.contains("authored submit reached"), "{report}");
    let capture = image::open(&artifacts.screenshot).unwrap().to_rgba8();
    let green = capture
        .pixels()
        .filter(|pixel| pixel[0] < 40 && pixel[1] > 130 && pixel[1] < 200 && pixel[2] < 70)
        .count();
    assert!(
        green > 1000,
        "authored child was covered or discarded: {green} green pixels"
    );
}
