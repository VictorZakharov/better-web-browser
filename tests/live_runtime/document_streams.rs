//! In-place replacement must work in the retained renderer, including after the original load.
use super::super::support::*;
use std::{fs, net::TcpListener, thread, time::Duration};

fn run(source: &'static str) -> serde_json::Value {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/streams", listener.local_addr().unwrap());
    let server = thread::spawn(move || serve_fixtures(listener, 1, |_| source));
    let artifacts = TestArtifacts::new();
    let mut child = hidden_benchmark(&url, &artifacts, 900);
    assert!(wait_for_child(&mut child, Duration::from_secs(20)).success());
    server.join().unwrap().unwrap();
    let report: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&artifacts.json).unwrap()).unwrap();
    assert!(
        report["javascript_errors"].as_array().unwrap().is_empty(),
        "{report}"
    );
    assert_eq!(report["javascript_runtime_stopped"], false, "{report}");
    report["titles"]["document_title"].clone()
}

fn run_resources(count: usize, response: fn(&str) -> FixtureResponse) -> serde_json::Value {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/streams", listener.local_addr().unwrap());
    let server = thread::spawn(move || serve_parallel_fixtures(listener, count, response));
    let artifacts = TestArtifacts::new();
    let mut child = hidden_benchmark(&url, &artifacts, 1400);
    assert!(wait_for_child(&mut child, Duration::from_secs(20)).success());
    server.join().unwrap().unwrap();
    let report: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&artifacts.json).unwrap()).unwrap();
    assert!(
        report["javascript_errors"].as_array().unwrap().is_empty(),
        "{report}"
    );
    assert_eq!(report["javascript_runtime_stopped"], false, "{report}");
    report["titles"]["document_title"].clone()
}

#[test]
fn failed_written_script_unblocks_the_stream_and_load() {
    let title = run_resources(2, |request| {
        if request.starts_with("GET /missing.js") {
            FixtureResponse::script("", Duration::from_millis(100)).status(404, "Not Found")
        } else {
            FixtureResponse::html(
                r#"<!doctype html><script>
              onload=()=>{window.order=[];document.open();
                document.write('<body><script src=/missing.js onerror="order.push(1)"><\/script><p id=tail>tail</p>');
                document.close();
                onload=()=>{order.push(document.getElementById('tail').textContent);document.title=JSON.stringify(order)};
              };
            </script>"#,
            )
        }
    });
    assert_eq!(title, r#"[1,"tail"]"#);
}

#[test]
fn replacement_cancels_old_parser_script_owners() {
    let title = run_resources(2, |request| {
        if request.starts_with("GET /old.js") {
            FixtureResponse::script("window.stale=true", Duration::from_millis(250))
        } else {
            FixtureResponse::html(
                r#"<!doctype html><script>
              setTimeout(()=>{document.open();document.write('<!doctype html><p>replacement</p>');
                document.close();setTimeout(()=>document.title=String(window.stale===undefined),400)},20);
            </script><script src=/old.js></script><script>window.stale=true</script>"#,
            )
        }
    });
    assert_eq!(title, "true");
}

#[test]
fn deferred_external_script_cannot_destructively_write() {
    let title = run_resources(2, |request| {
        if request.starts_with("GET /defer.js") {
            FixtureResponse::script(
                "document.write('<p>wrong</p>'); document.title=String(!!document.getElementById('keep'));",
                Duration::from_millis(100),
            )
        } else {
            FixtureResponse::html(
                "<!doctype html><script defer src=/defer.js></script><p id=keep>keep</p>",
            )
        }
    });
    assert_eq!(title, "true");
}

#[test]
fn replacement_stylesheet_blocks_written_script_and_new_load() {
    let title = run_resources(2, |request| {
        if request.starts_with("GET /new.css") {
            FixtureResponse::resource(
                "#probe{color:rgb(1,2,3)}",
                "text/css",
                Duration::from_millis(200),
            )
        } else {
            FixtureResponse::html(
                r#"<!doctype html><script>
              onload=()=>{window.order=[];document.open();
                document.write('<!doctype html><link rel=stylesheet href=/new.css><body><p id=probe>fresh</p><script>order.push(getComputedStyle(probe).color)<\/script><p id=tail>tail</p>');
                order.push(document.getElementById('tail')===null);document.close();
                onload=()=>{order.push(document.getElementById('tail').textContent);document.title=JSON.stringify(order)};
              };
            </script>"#,
            )
        }
    });
    assert_eq!(title, r#"[true,"rgb(1, 2, 3)","tail"]"#);
}

#[test]
fn replacing_again_from_readiness_does_not_finish_the_new_stream() {
    assert_eq!(
        run(r#"<!doctype html><script>
      onload=()=>{document.open();document.write('<p>first</p>');
        document.onreadystatechange=()=>{if(document.readyState==='interactive'){
          document.open();document.write('<p id=second>second</p>');
          setTimeout(()=>{document.title=JSON.stringify([document.readyState,!!document.getElementById('second')]);document.close()},30);
        }};
        document.close();
      };
    </script>"#),
        r#"["loading",true]"#
    );
}

#[test]
fn inert_stream_quirks_are_reset_and_recomputed() {
    assert_eq!(
        run(r#"<!doctype html><script>
      const d=document.implementation.createHTMLDocument(),log=[];
      d.open();log.push(d.compatMode);d.write('<p>quirks');d.close();log.push(d.compatMode);
      d.open();log.push(d.compatMode);d.write('<!doctype html><p>standards');d.close();log.push(d.compatMode);
      document.title=JSON.stringify(log);
    </script>"#),
        r#"["CSS1Compat","BackCompat","CSS1Compat","CSS1Compat"]"#
    );
}

#[test]
fn open_reuses_document_erases_listeners_and_reaches_a_new_load() {
    let title = run(r#"<!doctype html><body><p id=old>old</p><script>
    window.onload = () => {
        const doc=document, old=document.getElementById('old'), log=[];
        const detached=document.createElement('div');
        old.onclick=()=>log.push('old'); document.onclick=()=>log.push('document');
        window.addEventListener('click',()=>log.push('window'));
        detached.onclick=()=>log.push('detached');
        const result=document.open();
        log.push(result===doc,document.documentElement===null,document.readyState);
        old.click(); document.dispatchEvent(new Event('click')); window.dispatchEvent(new Event('click')); detached.click();
        document.write('<!doctype html><body><p id=new>fresh</p>');
        log.push(document.getElementById('new').textContent,old.parentNode===null);
        document.onreadystatechange=()=>log.push(document.readyState);
        document.addEventListener('DOMContentLoaded',()=>log.push('dcl'));
        window.onload=()=>{log.push('load');document.title=JSON.stringify(log)};
        document.close(); log.push('returned');
    };
    </script>"#);
    assert_eq!(
        title,
        r#"[true,true,"loading","detached","fresh",false,"interactive","returned","dcl","complete","load"]"#
    );
}

#[test]
fn post_load_write_replaces_and_split_tokens_are_synchronous() {
    let title = run(r#"<!doctype html><body><p id=old>old</p><script>
    window.onload=()=>{
        document.write('<body><p id=ne'); document.write('w>A&am'); document.write('p;B</p>');
        const log=[document.getElementById('old')===null,document.getElementById('new').textContent];
        document.writeln('line'); document.close();
        document.title=JSON.stringify(log);
    };
    </script>"#);
    assert_eq!(title, r#"[true,"A&B"]"#);
}

#[test]
fn parser_open_close_are_noops_and_xml_methods_throw() {
    let title = run(r#"<!doctype html><body><p id=old>old</p><script>
      const log=[document.open()===document,!!document.getElementById('old')];
      document.close(); log.push(document.readyState);
      const xml=document.implementation.createDocument('','root');
      for(const method of ['open','close','write','writeln']) {
        try { xml[method]('text'); } catch(e) { log.push(e.name); }
      }
      document.title=JSON.stringify(log);
    </script><p id=tail>tail</p>"#);
    assert_eq!(
        title,
        r#"[true,true,"loading","InvalidStateError","InvalidStateError","InvalidStateError","InvalidStateError"]"#
    );
}

#[test]
fn inert_document_streams_do_not_replace_primary_or_execute_scripts() {
    let title = run(r#"<!doctype html><body><script>
      const other=document.implementation.createHTMLDocument();
      other.open(); other.write('<body><p id=x>A&amp;B</p><script>throw new Error("inert")<\/script>');
      other.close();
      document.title=JSON.stringify([other.getElementById('x').textContent,document.getElementById('x')===null]);
    </script>"#);
    assert_eq!(title, r#"["A&B",true]"#);
}

#[test]
fn close_inside_nested_script_finishes_only_after_remaining_written_input() {
    let title = run(r#"<!doctype html><body><script>
      window.onload=()=>{
        window.order=[];
        document.open();
        document.write('<body><script>document.close(); order.push(document.readyState);<\/script><p id=tail>tail</p>');
        order.push(document.getElementById('tail').textContent,document.readyState);
        document.title=JSON.stringify(order);
      };
    </script>"#);
    assert_eq!(title, r#"["loading","tail","interactive"]"#);
}

#[test]
fn replacement_waits_for_external_script_and_preserves_nested_write_order() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/streams", listener.local_addr().unwrap());
    let server = thread::spawn(move || {
        serve_parallel_fixtures(listener, 2, |request| {
            if request.starts_with("GET /external.js") {
                FixtureResponse::script(
                    "order.push('external'); document.write('<b id=child>child</b>'); order.push(document.getElementById('child')?.textContent || 'missing');",
                    Duration::from_millis(200),
                )
            } else {
                FixtureResponse::html(
                    r#"<!doctype html><body><script>
            window.onload=()=>{
                window.order=[];
                document.open();
                document.write('<body><script src=/external.js><\/script><p id=tail>tail</p>');
                order.push(document.getElementById('tail')===null);
                document.close(); order.push(document.readyState);
                window.onload=()=>{order.push(document.getElementById('tail').textContent);document.title=JSON.stringify(order)};
            };
            </script>"#,
                )
            }
        })
    });
    let artifacts = TestArtifacts::new();
    let mut child = hidden_benchmark(&url, &artifacts, 1400);
    assert!(wait_for_child(&mut child, Duration::from_secs(20)).success());
    server.join().unwrap().unwrap();
    let report: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&artifacts.json).unwrap()).unwrap();
    assert!(
        report["javascript_errors"].as_array().unwrap().is_empty(),
        "{report}"
    );
    assert_eq!(
        report["titles"]["document_title"], r#"[true,"loading","external","child","tail"]"#,
        "{report}"
    );
}
