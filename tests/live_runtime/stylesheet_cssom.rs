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

#[test]
fn inline_style_update_survives_overridden_element_methods_and_paints_content() {
    const HTML: &str = r#"<!doctype html><title>not updated</title><style>
        body { margin: 0 }
        #panel { width: 300px; height: 200px; background: rgb(220, 20, 20) }
    </style><div id=panel>Visible component</div><script>
        const panel = document.getElementById('panel');
        panel.setAttribute = panel.getAttribute = () => { throw Error('author method called'); };
        panel.style.backgroundColor = 'rgb(17, 170, 34)';
        document.title = panel.style.backgroundColor;
    </script>"#;
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/component", listener.local_addr().unwrap());
    let server = thread::spawn(move || serve_fixtures(listener, 1, |_| HTML));
    let artifacts = TestArtifacts::new();
    let mut child = hidden_benchmark(&url, &artifacts, 1000);
    assert!(wait_for_child(&mut child, Duration::from_secs(20)).success());
    server.join().unwrap().unwrap();

    let report: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&artifacts.json).unwrap()).unwrap();
    assert_eq!(report["titles"]["document_title"], "rgb(17, 170, 34)");
    assert!(report["retained_draw_items"].as_u64().unwrap() > 0);
    assert!(report["javascript_errors"].as_array().unwrap().is_empty());
    let capture = image::open(&artifacts.screenshot).unwrap().to_rgba8();
    let green = capture
        .pixels()
        .filter(|pixel| pixel[0] < 40 && pixel[1] > 130 && pixel[1] < 200 && pixel[2] < 70)
        .count();
    assert!(
        green > 1000,
        "inline style did not paint: {green} green pixels"
    );
}
