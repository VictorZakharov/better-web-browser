use super::*;

#[test]
fn parser_mutations_invalidate_cssom_and_synchronous_layout_before_the_next_script() {
    let _serial = SERIAL.lock().unwrap_or_else(|error| error.into_inner());
    let mut driver = Driver::new(
        r#"<!doctype html><div id=box style='width:10px;height:10px;color:rgb(1,2,3)'></div><script>
        window.box=document.querySelector('#box');
        window.initialColor=getComputedStyle(box).color;
        window.initialWidth=box.getBoundingClientRect().width;
        </script><style>#box {height:20px !important;color:rgb(4,5,6) !important}</style>
        <div id=fresh style='width:33px;height:14px'></div><p id=status>waiting</p><script>
        const fresh=document.querySelector('#fresh');
        document.querySelector('#status').textContent='geometry:'+JSON.stringify([
          initialColor,initialWidth,getComputedStyle(box).color,box.getBoundingClientRect().height,
          fresh.getBoundingClientRect().width,fresh.offsetHeight]);
        </script>"#,
    );
    let result = painted_text(&driver.until_text("geometry:"));
    assert!(
        result.contains(r#"geometry:["rgb(1, 2, 3)",10,"rgb(4, 5, 6)",20,33,14]"#),
        "{result}"
    );
    driver.session.shutdown().unwrap();
}

#[test]
fn newly_parsed_elements_update_cached_children_and_upgrade_before_the_following_script() {
    let _serial = SERIAL.lock().unwrap_or_else(|error| error.into_inner());
    let mut driver = Driver::new(
        r#"<!doctype html><div id=box><script>
      window.box=document.querySelector('#box'); window.children=box.childNodes;
      customElements.define('x-parsed', class extends HTMLElement { constructor(){super();this.setAttribute('upgraded','yes');} });
      </script><x-parsed id=created></x-parsed></div><p id=status>pending</p><script>
      const child=document.querySelector('#created');
      document.querySelector('#status').textContent=box.childNodes===children && Array.from(children).includes(child) && child.getAttribute('upgraded')==='yes' ? 'parsed DOM refreshed':'WRONG';
      </script>"#,
    );
    driver.until_text("parsed DOM refreshed");
    driver.session.shutdown().unwrap();
}

#[test]
fn blocking_script_sees_only_the_parsed_prefix_and_does_not_block_heartbeats() {
    let _serial = SERIAL.lock().unwrap_or_else(|error| error.into_inner());
    let mut driver = Driver::new(
        r#"<!doctype html><div id=status>prefix visible</div>
      <script>window.trace=[];window.mark=x=>{trace.push(x);document.querySelector('#status').textContent=trace.join('|')};
      mark(document.querySelector('#tail')===null?'prefix':'WRONG');
      document.addEventListener('DOMContentLoaded',()=>mark('DCL'));</script>
      <script src=/block.js></script><div id=tail>tail visible</div>
      <script>mark('tail-'+document.readyState);</script>"#,
    );
    let prefix = driver.until_text("prefix");
    assert!(!painted_text(&prefix).contains("tail visible"));
    driver.advance();
    driver.until_idle();
    driver.session.ping(Duration::from_secs(1)).unwrap();
    driver.respond("block.js", "mark('block-'+(document.querySelector('#tail')===null)+'-'+document.readyState);Promise.resolve().then(()=>mark('micro'));document.currentScript.onload=()=>mark('load');", 200);
    driver.until_text("prefix|block-true-loading|micro|load|tail-loading|DCL");
    driver.session.shutdown().unwrap();
}

#[test]
fn speculative_completion_cannot_execute_a_script_before_the_parser_reaches_it() {
    let _serial = SERIAL.lock().unwrap_or_else(|error| error.into_inner());
    let mut driver = Driver::new(
        r#"<!doctype html><div id=status>waiting</div>
      <script>window.trace=[];window.mark=x=>{trace.push(x);document.querySelector('#status').textContent=trace.join('|')};</script>
      <script async src=/early.js></script><script src=/block.js></script>
      <script async src=/late.js></script><script>mark('tail');</script>"#,
    );
    driver.until_text("waiting");
    driver.respond("late.js", "mark('late');", 200);
    driver.respond("early.js", "mark('early');", 200);
    let prefix = driver.until_text("early");
    assert!(!painted_text(&prefix).contains("late"));
    driver.advance();
    driver.until_idle();
    driver.respond("block.js", "mark('block');", 200);
    driver.until_text("early|block|tail|late");
    driver.session.shutdown().unwrap();
}

#[test]
fn failed_blocking_fetch_resumes_parsing_and_delivers_error_before_following_script() {
    let _serial = SERIAL.lock().unwrap_or_else(|error| error.into_inner());
    let mut driver = Driver::new(
        r#"<!doctype html><div id=status>waiting</div>
      <script>window.failed=false;document.addEventListener('error',e=>{failed=e.isTrusted},true);</script>
      <script src=/missing.js></script>
      <script>document.querySelector('#status').textContent=failed?'failure released parser':'WRONG';</script>"#,
    );
    driver.until_text("waiting");
    driver.respond("missing.js", "missing", 404);
    driver.until_text("failure released parser");
    driver.session.shutdown().unwrap();
}

#[test]
fn stylesheet_wait_suspends_blocking_script_without_preventing_async_execution() {
    let _serial = SERIAL.lock().unwrap_or_else(|error| error.into_inner());
    let mut driver = Driver::new(
        r#"<!doctype html><div id=status>waiting</div>
      <script>window.trace=[];window.mark=x=>{trace.push(x);document.querySelector('#status').textContent=trace.join('|')};</script>
      <script async src=/fast.js></script><link rel=stylesheet href=/slow.css>
      <script>mark('inline');</script><script defer src=/defer.js></script>"#,
    );
    driver.until_text("waiting");
    driver.respond("fast.js", "mark('async');", 200);
    driver.until_text("async");
    driver.advance();
    driver.until_idle();
    driver.respond("defer.js", "mark('defer-'+document.readyState);", 200);
    driver.respond("slow.css", "#status { color: green; }", 200);
    driver.until_text("async|inline|defer-interactive");
    driver.session.shutdown().unwrap();
}

#[test]
fn buffered_parser_writes_keep_the_tree_builder_insertion_point_and_written_scripts() {
    let _serial = SERIAL.lock().unwrap_or_else(|error| error.into_inner());
    let mut driver = Driver::new(
        r##"<!doctype html><div id=box><script>
      document.write('<span id="written">one</span><script>window.writtenSawTail=!!document.querySelector("#tail");<\/script>');
      </script><span id=tail>two</span></div><div id=status>waiting</div><script>
      const box=document.querySelector('#box');
      document.querySelector('#status').textContent=
        document.querySelector('#written').parentNode===box && !writtenSawTail && box.textContent.includes('one') ? 'insertion point preserved':'WRONG';
      </script>"##,
    );
    driver.until_text("insertion point preserved");
    driver.session.shutdown().unwrap();
}

#[test]
fn script_preparation_uses_current_base_not_a_later_unparsed_base_element() {
    let _serial = SERIAL.lock().unwrap_or_else(|error| error.into_inner());
    let mut driver = Driver::new(
        r#"<!doctype html><div id=status>waiting</div>
      <script src=first.js></script><base href=/later/><script src=second.js></script>"#,
    );
    driver.until_text("waiting");
    assert!(
        driver
            .requests
            .keys()
            .any(|url| url.ends_with("/first.js") && !url.contains("/later/"))
    );
    driver.respond(
        "/first.js",
        "document.querySelector('#status').textContent='first ready';",
        200,
    );
    driver.until_text("first ready");
    driver.respond(
        "/later/second.js",
        "document.querySelector('#status').textContent='second ready';",
        200,
    );
    driver.until_text("second ready");
    driver.session.shutdown().unwrap();
}
