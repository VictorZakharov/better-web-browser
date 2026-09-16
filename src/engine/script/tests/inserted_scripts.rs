use super::*;

fn result(source: &str) -> String {
    let (dom, outcome) = execute_html(source);
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    dom.elements_named("body")
        .next()
        .unwrap()
        .attr("data-result")
        .unwrap()
}

#[test]
fn inserted_scripts_run_before_return_and_restore_the_outer_script() {
    assert_eq!(
        result(
            r#"<!doctype html><body><script id=outer>
      const trace=[]; window.trace=trace;
      const s=document.createElement('script'); s.id='inner';
      s.text='trace.push(document.currentScript.id); Promise.resolve().then(()=>trace.push("job"))';
      document.body.append(s); trace.push(document.currentScript.id);
      s.remove(); document.body.append(s);
      document.body.dataset.result=JSON.stringify(trace);
    </script>"#
        ),
        r#"["inner","outer"]"#
    );
}

#[test]
fn post_connection_steps_see_the_complete_atomic_insertion() {
    assert_eq!(
        result(
            r#"<!doctype html><body><p id=old></p><script>
      window.trace=[];
      const make=(id,code)=>{let s=document.createElement('script');s.id=id;s.text=code;return s};
      const f=document.createDocumentFragment();
      f.append(make('first','trace.push(!!document.getElementById("last"));document.getElementById("last").remove()'),
        make('last','trace.push("removed sibling ran")'));
      document.body.append(f);
      document.body.replaceChild(make('replacement','trace.push(!document.getElementById("old"))'),old);
      const box=document.createElement('div');document.body.append(box);box.innerHTML='<b id=gone></b>';
      box.replaceChildren(make('replacement2','trace.push(!document.getElementById("gone"))'));
      document.body.dataset.result=JSON.stringify(trace);
    </script>"#
        ),
        "[true,true,true]"
    );
}

#[test]
fn connected_empty_scripts_prepare_on_child_text_change_but_not_twice() {
    assert_eq!(
        result(
            r#"<!doctype html><body><script>
      window.trace=[];
      const s=document.createElement('script'); document.body.append(s);
      s.append(document.createComment('throw Error("comment")'));
      const text=document.createTextNode(''); s.append(text);
      text.data='trace.push("text")'; text.data='trace.push("twice")';
      const t=document.createElement('script'); document.body.append(t);
      t.replaceChildren('trace.push("replace")');
      const inert=document.createElement('div'); inert.innerHTML='<script>trace.push("inert")<\/script>';
      document.body.append(inert); document.body.append(inert.firstChild.cloneNode(true));
      const data=document.createElement('script');data.type='application/json';data.text='invalid javascript';document.body.append(data);
      const skip=document.createElement('script');skip.noModule=true;skip.text='trace.push("nomodule")';document.body.append(skip);
      document.body.dataset.result=JSON.stringify(trace);
    </script>"#
        ),
        r#"["text","replace"]"#
    );
}

#[test]
fn inserted_exceptions_are_reported_without_escaping_append_child() {
    assert_eq!(
        result(
            r#"<!doctype html><body><script id=outer>
      window.trace=[]; window.onerror=()=>{trace.push('error');return true};
      for(const code of ['throw Error("bad")','invalid } syntax']) {
        const s=document.createElement('script');s.text=code;document.body.append(s);
        trace.push(document.currentScript.id);
      }
      document.body.dataset.result=JSON.stringify(trace);
    </script>"#
        ),
        r#"["error","outer","error","outer"]"#
    );
}

#[test]
fn inline_async_defer_and_shadow_root_current_script_follow_classic_rules() {
    assert_eq!(
        result(
            r#"<!doctype html><body><script>
      window.trace=[];
      const s=document.createElement('script');s.async=true;s.defer=true;s.text='trace.push("sync")';
      document.body.append(s);trace.push('return');
      const host=document.createElement('div');document.body.append(host);
      const shadow=host.attachShadow({mode:'open'});
      const t=document.createElement('script');t.text='trace.push(document.currentScript===null)';shadow.append(t);
      const other=document.implementation.createHTMLDocument();
      const u=other.createElement('script');u.text='trace.push("inactive document")';other.body.append(u);
      document.body.dataset.result=JSON.stringify(trace);
    </script>"#
        ),
        r#"["sync","return",true]"#
    );
}

#[test]
fn recursive_inline_insertion_stops_at_the_shared_script_count_limit() {
    assert_eq!(
        result(
            r#"<!doctype html><body><script>
      window.count=0;window.errors=[];
      window.onerror=(message,source,line,column,error)=>{errors.push(error.name);return true};
      window.again=()=>{count++;const s=document.createElement('script');s.text='again()';document.body.append(s)};
      again();document.body.dataset.result=JSON.stringify([count,errors]);
    </script>"#
        ),
        r#"[64,["RangeError"]]"#
    );
}
