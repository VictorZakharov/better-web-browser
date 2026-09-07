use super::*;

#[test]
fn a_new_link_to_an_already_loaded_stylesheet_gets_its_own_async_load_event() {
    const HTML: &str = r#"<!doctype html><div id=target>ready</div><script>
      let firstLoads=0, secondLoads=0;
      const first=document.createElement('link');
      first.rel='stylesheet'; first.href='/shared.css';
      first.onload=()=>{
        firstLoads++;
        const second=document.createElement('link');
        second.rel='stylesheet'; second.href='/shared.css';
        let appended=false;
        second.onload=()=>{
          secondLoads++;
          console.log('second stylesheet loaded asynchronously: '+appended);
          console.log('cached stylesheet color: '+getComputedStyle(document.getElementById('target')).color);
        };
        document.head.appendChild(second);
        appended=true;
      };
      setTimeout(()=>document.head.appendChild(first),100);
      setTimeout(()=>console.log('stylesheet event counts: '+firstLoads+','+secondLoads),1000);
    </script>"#;
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/styles", listener.local_addr().unwrap());
    let server = thread::spawn(move || {
        serve_parallel_fixtures(listener, 2, |request| {
            if request.contains("GET /shared.css ") {
                FixtureResponse::streamed(
                    "text/css",
                    vec![b"#target {color:#123456}".to_vec()],
                    Duration::ZERO,
                )
            } else {
                FixtureResponse::html(HTML)
            }
        })
    });
    let artifacts = TestArtifacts::new();
    let mut child = hidden_benchmark(&url, &artifacts, 1800);
    assert!(wait_for_child(&mut child, Duration::from_secs(20)).success());
    server.join().unwrap().unwrap();
    let report = fs::read_to_string(&artifacts.json).unwrap();
    for expected in [
        "second stylesheet loaded asynchronously: true",
        "cached stylesheet color: rgb(18, 52, 86)",
        "stylesheet event counts: 1,1",
    ] {
        assert!(report.contains(expected), "missing {expected}: {report}");
    }
}

#[test]
fn external_stylesheet_load_handler_observes_computed_style_and_geometry() {
    const HTML: &str = r#"<!doctype html><body><div id=target></div><script>
      setTimeout(() => {
        const link = document.createElement('link');
        link.rel = 'stylesheet'; link.href = '/late.css';
        link.onload = () => {
          const target = document.getElementById('target');
          const style = getComputedStyle(target);
          if (style.position === 'relative' && style.color === 'rgb(18, 52, 86)' &&
              target.getBoundingClientRect().width === 123)
            console.log('external CSSOM and geometry agree');
          else console.log('external CSSOM mismatch: ' + style.position + '/' + style.color +
            '/' + target.getBoundingClientRect().width);
        };
        document.head.appendChild(link);
      }, 100);
    </script>"#;
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/styles", listener.local_addr().unwrap());
    let server = thread::spawn(move || {
        serve_parallel_fixtures(listener, 2, |request| {
            if request.contains("GET /late.css ") {
                FixtureResponse::streamed(
                    "text/css",
                    vec![
                        b"#target {position:relative;color:#123456;width:123px;height:20px}"
                            .to_vec(),
                    ],
                    Duration::ZERO,
                )
            } else {
                FixtureResponse::html(HTML)
            }
        })
    });
    let artifacts = TestArtifacts::new();
    let mut child = hidden_benchmark(&url, &artifacts, 1400);
    assert!(wait_for_child(&mut child, Duration::from_secs(20)).success());
    server.join().unwrap().unwrap();
    let report = fs::read_to_string(&artifacts.json).unwrap();
    assert!(
        report.contains("external CSSOM and geometry agree"),
        "{report}"
    );
}
