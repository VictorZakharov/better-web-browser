use super::*;

#[test]
fn inserting_an_import_loads_it_and_mutates_only_its_own_occurrence() {
    const HTML: &str = r#"<!doctype html><style id=left></style><style id=right></style>
    <p id=target>test</p><script>
    addEventListener('load',()=>{
      left.sheet.insertRule('@import "/nested.css";');
      right.sheet.insertRule('@import "/nested.css";');
      const poll=setInterval(()=>{
        const a=left.sheet.cssRules[0].styleSheet, b=right.sheet.cssRules[0].styleSheet;
        if(!a||!b||getComputedStyle(target).color!=='rgb(255, 0, 0)')return;
        clearInterval(poll);
        b.cssRules[0].style.color='blue';
        document.title=JSON.stringify([a!==b,a.cssRules[0].style.color,
          getComputedStyle(target).color]);
      },20);
    });
    </script>"#;
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/cssom", listener.local_addr().unwrap());
    let server = thread::spawn(move || {
        serve_parallel_fixtures(listener, 2, |request| {
            if request.contains("GET /nested.css ") {
                FixtureResponse::streamed(
                    "text/css",
                    vec![b"#target{color:red}".to_vec()],
                    Duration::from_millis(50),
                )
            } else {
                FixtureResponse::html(HTML)
            }
        })
    });
    let artifacts = TestArtifacts::new();
    let mut child = hidden_benchmark(&url, &artifacts, 1300);
    assert!(wait_for_child(&mut child, Duration::from_secs(20)).success());
    server.join().unwrap().unwrap();
    let report: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&artifacts.json).unwrap()).unwrap();
    assert_eq!(
        report["titles"]["document_title"], r#"[true,"red","rgb(0, 0, 255)"]"#,
        "{report}"
    );
    assert!(
        report["javascript_errors"].as_array().unwrap().is_empty(),
        "{report}"
    );
}
