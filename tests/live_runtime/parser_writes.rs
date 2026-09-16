//! Parser re-entry is verified in the production retained-parser path, not a completed DOM.
use super::support::*;
use std::{fs, net::TcpListener, thread, time::Duration};

fn run(source: &'static str) -> serde_json::Value {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/writes", listener.local_addr().unwrap());
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
    report
}

#[test]
fn writes_are_visible_before_return_without_consuming_the_network_tail() {
    let report = run(r#"<!doctype html><body><script id=outer>
      const result=[];
      document.write('<section id=written><b>now</b></section>');
      result.push(document.getElementById('written')?.textContent || 'missing');
      result.push(document.getElementById('tail') === null);
      document.write('<p id=split>A&am'); document.write('p;B</p>');
      result.push(document.getElementById('split')?.textContent);
      document.title=JSON.stringify(result);
    </script><div id=tail>tail</div>"#);
    assert_eq!(
        report["titles"]["document_title"], r#"["now",true,"A&B"]"#,
        "{report}"
    );
}

#[test]
fn write_arguments_use_domstring_conversion_once_before_parsing() {
    let report = run(r#"<!doctype html><body><script>
      const hints=[];
      const value={ [Symbol.toPrimitive](hint){ hints.push(hint); return '<i id=ok>ok</i>'; }};
      document.write(value,null,undefined);
      try { document.write('<b id=bad>bad</b>',Symbol()); }
      catch(e) { hints.push(e.name); }
      hints.push(document.getElementById('ok').textContent,document.getElementById('bad')===null);
      document.title=JSON.stringify(hints);
    </script>"#);
    assert_eq!(
        report["titles"]["document_title"], r#"["string","TypeError","ok",true]"#,
        "{report}"
    );
}

#[test]
fn writes_use_table_tree_building_and_refresh_live_collections() {
    let report = run(r#"<!doctype html><body><table id=t><script>
      const rows=document.getElementsByTagName('tr'); const before=rows.length;
      document.write('<tr><td id=cell>inside</td></tr>');
      const cell=document.getElementById('cell');
      document.title=JSON.stringify([before,rows.length,cell.textContent,
        cell.parentElement.parentElement.localName,cell.closest('table').id]);
    </script></table>"#);
    assert_eq!(
        report["titles"]["document_title"], r#"[0,1,"inside","tbody","t"]"#,
        "{report}"
    );
}

#[test]
fn dynamic_stylesheet_does_not_pause_a_written_inline_script() {
    let report = run(r#"<!doctype html><body><script>
      const link=document.createElement('link'); link.rel='stylesheet'; link.href='/late.css';
      document.head.appendChild(link);
      window.order=['outer'];
      document.write('<script>order.push("inner");<\/script>');
      order.push('return'); document.title=JSON.stringify(order);
    </script>"#);
    assert_eq!(
        report["titles"]["document_title"], r#"["outer","inner","return"]"#,
        "{report}"
    );
}

#[test]
fn parser_custom_element_constructor_cannot_reenter_dynamic_markup_insertion() {
    let report = run(r#"<!doctype html><body><script>
      window.result=[];
      customElements.define('write-test',class extends HTMLElement {
        constructor(){super(); try { document.write('<i>bad</i>'); }
          catch(e) { result.push(e.name); }}
        connectedCallback(){result.push('connected');}
      });
      document.write('<write-test></write-test>');
      document.write('<b id=after>after</b>');
      result.push(document.getElementsByTagName('i').length,document.getElementById('after').textContent);
      document.title=JSON.stringify(result);
    </script>"#);
    assert_eq!(
        report["titles"]["document_title"], r#"["InvalidStateError","connected",0,"after"]"#,
        "{report}"
    );
}

#[test]
fn a_write_depth_limit_still_releases_document_lifecycle() {
    let report = run(r#"<!doctype html><body><p>safe prefix</p><script>
      window.addEventListener('load',()=>document.title='limited complete');
      document.write('<div>'.repeat(20000));
      document.title='limited';
    </script>"#);
    assert_eq!(
        report["titles"]["document_title"], "limited complete",
        "{report}"
    );
}

#[test]
fn recursive_writes_hit_a_bounded_error_without_stopping_the_renderer() {
    let report = run(r#"<!doctype html><body><script>
      window.depth=0; window.errors=[];
      window.onerror=(m,s,l,c,e)=>{errors.push(e.name); return true;};
      function recurse(){depth++; document.write('<script>recurse();<\/script>');}
      recurse();
      document.write('<p id=after>survived</p>');
      document.title=JSON.stringify([depth,errors,document.getElementById('after').textContent]);
    </script>"#);
    assert_eq!(
        report["titles"]["document_title"], r#"[33,["RangeError"],"survived"]"#,
        "{report}"
    );
}

#[test]
fn nested_written_scripts_run_on_the_call_stack_and_restore_current_script() {
    let report = run(r#"<!doctype html><body><script id=outer>
      window.order=['outer'];
      document.write('<script id=inner>order.push(document.currentScript.id);' +
        'document.write("<i id=nested>child</i>");' +
        'order.push(document.getElementById("nested").textContent);' +
        'Promise.resolve().then(()=>order.push("micro"));<\/script><b id=after>after</b>');
      order.push(document.currentScript.id);
      order.push(document.getElementById('after')?.textContent || 'missing');
      order.push('return');
    </script><script>document.title=JSON.stringify(order)</script>"#);
    assert_eq!(
        report["titles"]["document_title"],
        r#"["outer","inner","child","outer","after","return","micro"]"#,
        "{report}"
    );
}

#[test]
fn nested_exceptions_report_errors_but_do_not_abort_the_writing_script() {
    let report = run(r#"<!doctype html><body><script id=outer>
      window.order=[];
      window.onerror=()=>{order.push('error'); return true;};
      document.write('<script>throw new Error("nested");<\/script><p id=after>ok</p>');
      order.push(document.currentScript.id,document.getElementById('after').textContent);
      document.write('<script>invalid } syntax<\/script>');
      order.push('alive');
      document.title=JSON.stringify(order);
    </script>"#);
    assert_eq!(
        report["titles"]["document_title"], r#"["error","outer","ok","error","alive"]"#,
        "{report}"
    );
}

#[test]
fn pending_written_external_script_pauses_parsing_but_not_the_caller() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/", listener.local_addr().unwrap());
    let server = thread::spawn(move || {
        serve_parallel_fixtures(listener, 2, |request| {
            if request.starts_with("GET /written.js") {
                FixtureResponse::script(
                    "order.push('external'); document.write('<i id=nested>child</i>'); order.push(document.getElementById('nested').textContent);",
                    Duration::from_millis(250),
                )
            } else {
                FixtureResponse::html(
                    r#"<!doctype html><body><script>
              window.order=['outer'];
              document.write('<script src=/written.js onload="order.push(document.currentScript===null);' +
                'document.write(&quot;<em id=load-child>loaded</em>&quot;);' +
                'order.push(document.getElementById(&quot;load-child&quot;).textContent)"><\/script><p id=first>first</p>');
              order.push(document.getElementById('first')===null);
              document.write('<p id=second>second</p>');
              order.push(document.getElementById('second')===null);
              order.push('return');
            </script><script>
              order.push([...document.querySelectorAll('p')].map(n=>n.id).join(','));
              document.title=JSON.stringify(order);
            </script>"#,
                )
            }
        })
    });
    let artifacts = TestArtifacts::new();
    let mut child = hidden_benchmark(&url, &artifacts, 1600);
    assert!(wait_for_child(&mut child, Duration::from_secs(20)).success());
    server.join().unwrap().unwrap();
    let report: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&artifacts.json).unwrap()).unwrap();
    assert!(
        report["javascript_errors"].as_array().unwrap().is_empty(),
        "{report}"
    );
    assert_eq!(
        report["titles"]["document_title"],
        r#"["outer",true,true,"return","external","child",true,"loaded","first,second"]"#,
        "{report}"
    );
}

#[test]
fn written_stylesheet_blocks_the_written_script_until_its_response_arrives() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/", listener.local_addr().unwrap());
    let server = thread::spawn(move || {
        serve_parallel_fixtures(listener, 2, |request| {
            if request.starts_with("GET /written.css") {
                FixtureResponse::resource(
                    "#probe { color: rgb(12, 34, 56) }",
                    "text/css",
                    Duration::from_millis(250),
                )
            } else {
                FixtureResponse::html(
                    r#"<!doctype html><body><p id=probe>probe</p><script>
              window.order=['outer'];
              document.write('<link rel=stylesheet href=/written.css><script>' +
                'order.push(getComputedStyle(document.getElementById("probe")).color);<\/script>');
              order.push('return');
            </script><script>document.title=JSON.stringify(order)</script>"#,
                )
            }
        })
    });
    let artifacts = TestArtifacts::new();
    let mut child = hidden_benchmark(&url, &artifacts, 1600);
    assert!(wait_for_child(&mut child, Duration::from_secs(20)).success());
    server.join().unwrap().unwrap();
    let report: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&artifacts.json).unwrap()).unwrap();
    assert!(
        report["javascript_errors"].as_array().unwrap().is_empty(),
        "{report}"
    );
    assert_eq!(
        report["titles"]["document_title"], r#"["outer","return","rgb(12, 34, 56)"]"#,
        "{report}"
    );
}
